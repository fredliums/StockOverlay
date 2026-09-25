"""Refresh the bundled stock search index from public exchange data.

Run with Python 3.11+: py -3.11 scripts/update_stock_index.py
The generated JSON is reviewed and committed with source code; the app makes no
search-time directory requests. No third-party Python packages are required.
"""

from __future__ import annotations

import html
import json
import re
import time
import urllib.parse
import urllib.request
import zipfile
from io import BytesIO
from pathlib import Path
from xml.etree import ElementTree


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "src-tauri" / "resources" / "stocks.json"
SSE_URL = "https://query.sse.com.cn/sseQuery/commonQuery.do"
SZSE_URL = "https://www.szse.cn/api/report/ShowReport"
BSE_MAPPING_URL = "https://www.bseinfo.net/service/code_mapping.html"
TENCENT_URL = "https://qt.gtimg.cn/q="
USER_AGENT = "curl/8.21.0"


def fetch(url: str, *, referer: str | None = None) -> bytes:
    headers = {"User-Agent": USER_AGENT}
    if referer:
        headers["Referer"] = referer
    request = urllib.request.Request(url, headers=headers)
    with urllib.request.urlopen(request, timeout=30) as response:
        return response.read()


def sse_stocks(stock_type: str) -> list[dict[str, str]]:
    params = {
        "STOCK_TYPE": stock_type,
        "REG_PROVINCE": "",
        "CSRC_CODE": "",
        "STOCK_CODE": "",
        "sqlId": "COMMON_SSE_CP_GPJCTPZ_GPLB_GP_L",
        "COMPANY_STATUS": "2,4,5,7,8",
        "type": "inParams",
        "isPagination": "true",
        "pageHelp.cacheSize": "1",
        "pageHelp.beginPage": "1",
        "pageHelp.pageSize": "10000",
        "pageHelp.pageNo": "1",
        "pageHelp.endPage": "1",
    }
    url = f"{SSE_URL}?{urllib.parse.urlencode(params)}"
    data = json.loads(fetch(url, referer="https://www.sse.com.cn/assortment/stock/list/share/"))
    rows = data["pageHelp"]["data"]
    if len(rows) != data["pageHelp"]["total"]:
        raise ValueError("SSE stock list was truncated")
    return [
        {"market": "SH", "code": row["A_STOCK_CODE"], "name": row["SEC_NAME_CN"].strip()}
        for row in rows
    ]


def szse_stocks() -> list[dict[str, str]]:
    params = urllib.parse.urlencode({"SHOWTYPE": "xlsx", "CATALOGID": "1110", "TABKEY": "tab1"})
    workbook = fetch(f"{SZSE_URL}?{params}", referer="https://www.szse.cn/market/product/stock/list/")
    with zipfile.ZipFile(BytesIO(workbook)) as archive:
        sheet = ElementTree.fromstring(archive.read("xl/worksheets/sheet1.xml"))
    ns = "{http://schemas.openxmlformats.org/spreadsheetml/2006/main}"
    rows = sheet.findall(f"{ns}sheetData/{ns}row")
    result = []
    for row in rows[1:]:
        values = {}
        for cell in row.findall(f"{ns}c"):
            column = re.match(r"[A-Z]+", cell.attrib["r"]).group()
            if column in {"E", "F"}:
                values[column] = "".join(node.text or "" for node in cell.iter(f"{ns}t"))
        code = values.get("E", "")
        if code:
            result.append({"market": "SZ", "code": code, "name": values.get("F", "").strip()})
    return result


def bse_old_codes() -> dict[str, tuple[str, str]]:
    page = fetch(BSE_MAPPING_URL).decode("utf-8")
    mapping = {}
    for row in re.findall(r"<tr\b[^>]*>(.*?)</tr>", page, flags=re.IGNORECASE | re.DOTALL):
        cells = [
            html.unescape(re.sub(r"<[^>]+>", "", cell)).strip()
            for cell in re.findall(r"<td\b[^>]*>(.*?)</td>", row, flags=re.IGNORECASE | re.DOTALL)
        ]
        cells = [re.sub(r"\s+", "", cell) for cell in cells]
        if len(cells) == 5 and re.fullmatch(r"\d{6}", cells[3]) and re.fullmatch(r"920\d{3}", cells[4]):
            old_code, new_code = cells[3], cells[4]
            if new_code in mapping:
                raise ValueError(f"duplicate BSE code: {new_code}")
            mapping[new_code] = (old_code, cells[1])
    return mapping


def bse_stocks(mapping: dict[str, tuple[str, str]]) -> list[dict[str, str]]:
    found = {}
    for start in range(920000, 921000, 50):
        codes = [str(code) for code in range(start, start + 50)]
        payload = fetch(TENCENT_URL + ",".join("bj" + code for code in codes)).decode("gbk")
        for code, body in re.findall(r'v_bj(920\d{3})="([^"]*)"', payload):
            fields = body.split("~")
            if len(fields) < 5 or fields[2] != code or not fields[1].strip():
                continue
            try:
                if float(fields[3]) <= 0:
                    continue
            except ValueError:
                continue
            row = {"market": "BJ", "code": code, "name": fields[1].strip()}
            if code in mapping:
                row["legacyCode"] = mapping[code][0]
            found[code] = row
        time.sleep(0.25)
    return list(found.values())


def main() -> None:
    main_board = sse_stocks("1")
    star_board = sse_stocks("8")
    shenzhen = szse_stocks()
    mapping = bse_old_codes()
    beijing = bse_stocks(mapping)
    if len(main_board) < 1500 or len(star_board) < 500 or len(shenzhen) < 2500:
        raise ValueError("an exchange stock list looks incomplete")
    if len(mapping) < 200 or len(beijing) < 200:
        raise ValueError("the Beijing stock list or mapping looks incomplete")
    stocks = sorted(main_board + star_board + shenzhen + beijing, key=lambda item: (item["market"], item["code"]))
    keys = [(item["market"], item["code"]) for item in stocks]
    if len(keys) != len(set(keys)):
        raise ValueError("duplicate market/code in stock index")
    for item in stocks:
        if not re.fullmatch(r"\d{6}", item["code"]) or not item["name"]:
            raise ValueError(f"invalid stock index row: {item}")
    if next((item for item in stocks if item["market"] == "SH" and item["code"] == "600519"), {}).get("name") != "贵州茅台":
        raise ValueError("the expected SSE stock is missing")
    if not any(item.get("legacyCode") == "834021" and item["code"] == "920021" for item in stocks):
        raise ValueError("the expected BSE old/new mapping is missing")
    OUTPUT.parent.mkdir(parents=True, exist_ok=True)
    temporary = OUTPUT.with_suffix(".json.tmp")
    temporary.write_text(json.dumps(stocks, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
    temporary.replace(OUTPUT)
    print(f"Wrote {len(stocks)} stocks: SH {len(main_board) + len(star_board)}, SZ {len(shenzhen)}, BJ {len(beijing)}")


if __name__ == "__main__":
    main()
