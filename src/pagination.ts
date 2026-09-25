export type StockPart = { stockIndex: number; start: number; end: number };

/** Pack complete cards first; split an oversized card only between its fields or book rows. */
export function paginateStocks(
  heights: number[],
  atomCounts: number[],
  availableHeight: number,
  measurePart: (stockIndex: number, start: number, end: number) => number,
): StockPart[][] {
  const pages: StockPart[][] = [[]];
  let used = 0;
  const gap = 8;
  const nextPage = () => {
    if (pages[pages.length - 1].length > 0) pages.push([]);
    used = 0;
  };
  const add = (part: StockPart, height: number) => {
    pages[pages.length - 1].push(part);
    used += height + (used > 0 ? gap : 0);
  };

  heights.forEach((fullHeight, stockIndex) => {
    const atoms = atomCounts[stockIndex];
    const whole = { stockIndex, start: 0, end: Infinity };
    if (used + (used > 0 ? gap : 0) + fullHeight <= availableHeight) {
      add(whole, fullHeight);
      return;
    }
    if (fullHeight <= availableHeight || atoms === 0) {
      nextPage();
      add(whole, fullHeight);
      return;
    }

    nextPage();
    let start = 0;
    while (start < atoms) {
      let end = start + 1;
      while (end < atoms && measurePart(stockIndex, start, end + 1) <= availableHeight) end += 1;
      add({ stockIndex, start, end }, measurePart(stockIndex, start, end));
      start = end;
      if (start < atoms) nextPage();
    }
  });
  return pages.filter((parts) => parts.length > 0);
}
