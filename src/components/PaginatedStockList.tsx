import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { AppConfig } from '../types/config';
import type { QuoteSnapshot } from '../types/quote';
import type { StockEntry } from '../types/stock';
import { paginateStocks, type StockPart } from '../pagination';
import { StockList } from './StockList';

function measurePart(card: HTMLElement, start: number, end: number, probe: HTMLElement): number {
  const copy = card.cloneNode(true) as HTMLElement;
  copy.querySelectorAll<HTMLElement>('[data-atom-index]').forEach((atom) => {
    const index = Number(atom.dataset.atomIndex);
    if (index < start || index >= end) atom.remove();
  });
  if (!copy.querySelector('.quote-field')) copy.querySelector('.quote-fields')?.remove();
  if (!copy.querySelector('.order-book__row')) copy.querySelector('.order-book')?.remove();
  copy.querySelectorAll('.order-book__side').forEach((side) => {
    if (!side.querySelector('.order-book__row')) side.remove();
  });
  if (!copy.querySelector('.order-book__side:not(.order-book__side--bid)')) {
    copy.querySelector('.order-book__side--bid')?.classList.remove('order-book__side--bid');
  }
  probe.replaceChildren(copy);
  return Math.ceil(copy.getBoundingClientRect().height);
}

function makePages(cards: HTMLElement[], probe: HTMLElement, availableHeight: number): StockPart[][] {
  const heights = cards.map((card) => Math.ceil(card.getBoundingClientRect().height));
  const atomCounts = cards.map((card) => card.querySelectorAll('[data-atom-index]').length);
  const pages = paginateStocks(heights, atomCounts, availableHeight,
    (stockIndex, start, end) => measurePart(cards[stockIndex], start, end, probe));
  probe.replaceChildren();
  return pages;
}

export function PaginatedStockList({
  config,
  snapshot,
  watchlist,
  locked,
}: {
  config: AppConfig;
  snapshot: QuoteSnapshot | null;
  watchlist: StockEntry[];
  locked: boolean;
}) {
  const editRef = useRef<HTMLDivElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const measureRef = useRef<HTMLDivElement>(null);
  const probeRef = useRef<HTMLDivElement>(null);
  const editScroll = useRef(0);
  const [pages, setPages] = useState<StockPart[][]>([]);
  const [page, setPage] = useState(0);

  useLayoutEffect(() => {
    if (!locked) {
      if (editRef.current) editRef.current.scrollTop = editScroll.current;
      return;
    }
    const viewport = viewportRef.current;
    const source = measureRef.current;
    const probe = probeRef.current;
    if (!viewport || !source || !probe) return;
    let frame = 0;
    const recalculate = () => {
      const cards = Array.from(source.querySelectorAll<HTMLElement>('.stock-card'));
      const available = Math.max(1, viewport.clientHeight - 10);
      const next = makePages(cards, probe, available);
      setPages(next);
      setPage((current) => Math.min(current, Math.max(0, next.length - 1)));
    };
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(recalculate);
    });
    observer.observe(viewport);
    observer.observe(source);
    frame = requestAnimationFrame(recalculate);
    return () => { observer.disconnect(); cancelAnimationFrame(frame); };
  }, [config, snapshot, watchlist, locked]);

  useEffect(() => {
    if (!locked || pages.length < 2) return;
    const timer = window.setInterval(() => setPage((current) => (current + 1) % pages.length), 8_000);
    return () => window.clearInterval(timer);
  }, [locked, pages.length]);

  return <div className={`stock-pages${locked ? ' stock-pages--locked' : ''}`}>
    <div className="stock-pages__edit" ref={editRef} onScroll={(event) => {
      if (!locked) editScroll.current = event.currentTarget.scrollTop;
    }}>
      <StockList config={config} snapshot={snapshot} watchlist={watchlist} />
    </div>
    {locked && <>
      <div className="stock-pages__viewport" ref={viewportRef}>
        <StockList config={config} snapshot={snapshot} watchlist={watchlist} parts={pages[page] ?? []} />
      </div>
      {pages.length > 1 && <div className="stock-pages__indicator">{page + 1} / {pages.length}</div>}
      <div className="stock-pages__measure" aria-hidden="true">
        <div ref={measureRef}><StockList config={config} snapshot={snapshot} watchlist={watchlist} /></div>
        <div className="stock-list" ref={probeRef} />
      </div>
    </>}
  </div>;
}
