import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type CSSProperties, type PointerEvent } from 'react';
import { appStore } from './appStore';
import { PaginatedStockList } from './components/PaginatedStockList';
import { WatchlistSettings } from './components/WatchlistSettings';
import { AppearanceSettings, DisplaySettings, QuoteSettings } from './components/DisplaySettings';
import { ShortcutSettings, type ShortcutStatus } from './components/ShortcutSettings';

function useAppState() {
  return useSyncExternalStore(appStore.subscribe, appStore.getState);
}

type ResizeDirection = 'North' | 'South' | 'East' | 'West' |
  'NorthEast' | 'NorthWest' | 'SouthEast' | 'SouthWest';
const resizeCursors: Record<ResizeDirection, string> = {
  North: 'ns-resize', South: 'ns-resize', East: 'ew-resize', West: 'ew-resize',
  NorthEast: 'nesw-resize', SouthWest: 'nesw-resize',
  NorthWest: 'nwse-resize', SouthEast: 'nwse-resize',
};

export default function App() {
  useEffect(() => appStore.connect(), []);
  return getCurrentWindow().label === 'settings' ? <SettingsShell /> : <OverlayApp />;
}

type SettingsTab = 'watchlist' | 'display' | 'appearance' | 'shortcuts' | 'quotes';
const settingsTabs: { id: SettingsTab; label: string }[] = [
  { id: 'watchlist', label: '自选股' },
  { id: 'display', label: '显示' },
  { id: 'appearance', label: '外观' },
  { id: 'shortcuts', label: '快捷键' },
  { id: 'quotes', label: '行情' },
];

function SettingsShell() {
  const { config, snapshot, watchlist, error } = useAppState();
  const [tab, setTab] = useState<SettingsTab>('watchlist');
  const [shortcuts, setShortcuts] = useState<ShortcutStatus | null>(null);
  useEffect(() => {
    let active = true;
    invoke<ShortcutStatus>('get_shortcut_status')
      .then((status) => { if (active) setShortcuts(status); })
      .catch(() => {});
    return () => { active = false; };
  }, []);
  return (
    <main className="settings-shell">
      <h1>StockOverlay 设置</h1>
      {config && snapshot && <p>{config.stocks.length} 只自选股 · {snapshot.quotes.length} 条行情</p>}
      <nav className="settings-shell__nav" aria-label="设置分类">
        {settingsTabs.map((item) => <button
          key={item.id}
          type="button"
          aria-current={tab === item.id ? 'page' : undefined}
          onClick={() => setTab(item.id)}
        >{item.label}</button>)}
      </nav>
      <section className="settings-shell__content" aria-label={`${settingsTabs.find((item) => item.id === tab)?.label}设置`}>
        {!config && <p>正在加载配置…</p>}
        {config && tab === 'watchlist' && <WatchlistSettings config={config} entries={watchlist} />}
        {config && tab === 'display' && <DisplaySettings config={config} />}
        {config && tab === 'appearance' && <AppearanceSettings config={config} />}
        {config && tab === 'quotes' && <QuoteSettings config={config} />}
        {config && tab === 'shortcuts' && <ShortcutSettings config={config} status={shortcuts} onStatusChange={setShortcuts} />}
      </section>
      {shortcuts?.lockError && <p role="alert">锁定快捷键不可用：{shortcuts.lockError}</p>}
      {shortcuts?.visibilityError && <p role="alert">显示快捷键不可用：{shortcuts.visibilityError}</p>}
      {error && <p role="alert">{error}</p>}
    </main>
  );
}

function OverlayApp() {
  const { config, snapshot, watchlist, locked } = useAppState();
  const overlayRef = useRef<HTMLElement>(null);
  const lastMinimum = useRef('');
  const appearance = config ? {
    '--quote-font-size': `${config.display.fontSize}px`,
    '--background-opacity': config.display.backgroundOpacity,
    '--text-opacity': config.display.textOpacity,
    '--rise-rgb': config.display.redForRise ? '255 105 98' : '74 207 142',
    '--fall-rgb': config.display.redForRise ? '74 207 142' : '255 105 98',
  } as CSSProperties : undefined;

  useLayoutEffect(() => {
    const overlay = overlayRef.current;
    if (!overlay) return;
    let frame = 0;
    const measure = () => {
      const atomic = overlay.querySelectorAll<HTMLElement>(
        '.quote-field, .order-book__row, .stock-card__identity > *, .stock-card__status',
      );
      const widest = Math.max(0, ...Array.from(atomic, (item) => item.scrollWidth));
      const tallest = Math.max(0, ...Array.from(atomic, (item) => item.getBoundingClientRect().height));
      const identity = overlay.querySelector<HTMLElement>('.stock-card__identity');
      const width = Math.ceil(widest + 54);
      const height = Math.ceil(64 + (identity?.offsetHeight ?? 0) + tallest);
      const key = `${width}:${height}`;
      if (key === lastMinimum.current) return;
      lastMinimum.current = key;
      invoke('set_content_min_size', { width, height }).catch((reason: unknown) => {
        reportWindowError('设置悬浮窗最小尺寸失败', reason);
      });
    };
    const observer = new ResizeObserver(() => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(measure);
    });
    observer.observe(overlay);
    frame = requestAnimationFrame(measure);
    return () => { observer.disconnect(); cancelAnimationFrame(frame); };
  }, [config, snapshot, watchlist, locked]);

  const resizeDirection = (event: PointerEvent<HTMLElement>): ResizeDirection | null => {
    const bounds = event.currentTarget.getBoundingClientRect();
    const left = event.clientX - bounds.left <= 12;
    const right = bounds.right - event.clientX <= 12;
    const top = event.clientY - bounds.top <= 12;
    const bottom = bounds.bottom - event.clientY <= 12;
    if (top && left) return 'NorthWest';
    if (top && right) return 'NorthEast';
    if (bottom && left) return 'SouthWest';
    if (bottom && right) return 'SouthEast';
    if (left) return 'West';
    if (right) return 'East';
    if (top) return 'North';
    if (bottom) return 'South';
    return null;
  };

  const pointerCursor = (event: PointerEvent<HTMLElement>) => {
    if (locked) return;
    const direction = resizeDirection(event);
    event.currentTarget.style.cursor = direction ? resizeCursors[direction] : 'grab';
  };

  const reportWindowError = (message: string, reason: unknown) => {
    console.error(message, reason);
    const detail = typeof reason === 'object' && reason !== null && 'message' in reason
      ? String(reason.message) : String(reason);
    void emit('window:error', `${message}：${detail}`).catch(console.error);
  };

  return (
    <main className="overlay" ref={overlayRef} style={appearance}
      onPointerMove={pointerCursor}
      onPointerLeave={(event) => { event.currentTarget.style.cursor = 'grab'; }}
      onPointerDown={(event) => {
        if (locked || event.button !== 0) return;
        const direction = resizeDirection(event);
        const operation = direction
          ? getCurrentWindow().startResizeDragging(direction)
          : getCurrentWindow().startDragging();
        operation.catch((reason: unknown) => reportWindowError('移动或调整悬浮窗失败', reason));
      }}
    >
      {config && <PaginatedStockList config={config} snapshot={snapshot} watchlist={watchlist} locked={locked} />}
    </main>
  );
}
