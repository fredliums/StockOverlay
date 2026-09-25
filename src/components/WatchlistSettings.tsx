import { useEffect, useState } from 'react';
import { appStore } from '../appStore';
import type { AppConfig } from '../types/config';
import type { StockEntry } from '../types/stock';

function messageOf(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error) return String(error.message);
  return String(error);
}

export function WatchlistSettings({ config, entries }: { config: AppConfig; entries: StockEntry[] }) {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<StockEntry[]>([]);
  const [searching, setSearching] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [dragged, setDragged] = useState<string | null>(null);

  useEffect(() => {
    const term = query.trim().replace(/^(sh|sz|bj)(?=\d)/i, '');
    setResults([]);
    if (!term) {
      setResults([]);
      setSearching(false);
      return;
    }
    let active = true;
    const timer = window.setTimeout(() => {
      setSearching(true);
      appStore.searchStocks(term)
        .then((found) => { if (active) setResults(found); })
        .catch((error: unknown) => { if (active) setMessage(messageOf(error)); })
        .finally(() => { if (active) setSearching(false); });
    }, 200);
    return () => { active = false; window.clearTimeout(timer); };
  }, [query]);

  const selected = new Set(config.stocks);
  const names = new Map(entries.map((entry) => [`${entry.market.toLowerCase()}${entry.code}`, entry.name]));
  const change = async (operation: () => Promise<unknown>, success: string) => {
    try {
      await operation();
      setMessage(success);
    } catch (error) {
      setMessage(messageOf(error));
    }
  };

  return <div className="watchlist-settings">
    <h2>自选股</h2>
    <label htmlFor="stock-search">按股票名称或六位代码搜索</label>
    <input id="stock-search" value={query} onChange={(event) => setQuery(event.target.value)} autoComplete="off" />
    {searching && <p>搜索中…</p>}
    {query.trim() && !searching && results.length === 0 && <p>未找到匹配的股票。</p>}
    {results.length > 0 && <ul className="settings-list" aria-label="搜索结果">
      {results.map((entry) => {
        const code = `${entry.market.toLowerCase()}${entry.code}`;
        return <li key={code}>
          <span>{entry.name} · {entry.market} {entry.code}{entry.legacyCode && `（旧代码 ${entry.legacyCode} → ${entry.code}）`}</span>
          <button type="button" disabled={selected.has(code)} onClick={() => void change(() => appStore.addStock(code), `已添加 ${entry.name}`)}>
            {selected.has(code) ? '已添加' : '添加'}
          </button>
        </li>;
      })}
    </ul>}
    <h3>已添加（拖动排序）</h3>
    {config.stocks.length === 0 && <p>尚无自选股。</p>}
    <ul className="settings-list" aria-label="自选股列表">
      {config.stocks.map((code) => <li
        key={code}
        draggable
        onDragStart={() => setDragged(code)}
        onDragEnd={() => setDragged(null)}
        onDragOver={(event) => event.preventDefault()}
        onDrop={(event) => {
          event.preventDefault();
          if (!dragged || dragged === code) return;
          const reordered = config.stocks.filter((item) => item !== dragged);
          reordered.splice(reordered.indexOf(code), 0, dragged);
          void change(() => appStore.reorderStocks(reordered), '已保存自选股顺序');
          setDragged(null);
        }}
      >
        <span className="settings-list__drag" aria-hidden="true">⠿</span>
        <span>{names.get(code) ?? code} · {code.slice(0, 2).toUpperCase()} {code.slice(2)}</span>
        <button type="button" onClick={() => void change(() => appStore.removeStock(code), `已删除 ${names.get(code) ?? code}`)}>删除</button>
      </li>)}
    </ul>
    {message && <p className="settings-shell__message" role="status">{message}</p>}
  </div>;
}
