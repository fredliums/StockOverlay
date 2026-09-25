import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type CSSProperties } from 'react';
import { appStore } from './appStore';
import { PaginatedStockList } from './components/PaginatedStockList';
import { WatchlistSettings } from './components/WatchlistSettings';
import { AppearanceSettings, DisplaySettings, QuoteSettings } from './components/DisplaySettings';
import { ShortcutSettings, type ShortcutStatus } from './components/ShortcutSettings';

function errorMessage(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error) {
    return String(error.message);
  }
  return String(error);
}

function useAppState() {
  return useSyncExternalStore(appStore.subscribe, appStore.getState);
}

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
  const { config, snapshot, watchlist, locked, error } = useAppState();
  const [windowError, setWindowError] = useState<string | null>(null);
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
      const toolbar = overlay.querySelector<HTMLElement>('.overlay__toolbar');
      const status = overlay.querySelector<HTMLElement>('.overlay__status');
      const width = Math.ceil(widest + 54);
      const height = Math.ceil(64 + (toolbar?.offsetHeight ?? 0) + (status?.offsetHeight ?? 0)
        + (identity?.offsetHeight ?? 0) + tallest);
      const key = `${width}:${height}`;
      if (key === lastMinimum.current) return;
      lastMinimum.current = key;
      invoke('set_content_min_size', { width, height }).catch((reason: unknown) => {
        setWindowError(`设置最小尺寸失败：${errorMessage(reason)}`);
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

  const startDrag = () => {
    getCurrentWindow().startDragging().catch((reason: unknown) => {
      setWindowError(`窗口拖动失败：${errorMessage(reason)}`);
    });
  };

  const startResize = () => {
    getCurrentWindow().startResizeDragging('SouthEast').catch((reason: unknown) => {
      setWindowError(`窗口调整大小失败：${errorMessage(reason)}`);
    });
  };

  const status = config && snapshot
    ? `${config.stocks.length} 只自选股 · ${snapshot.quotes.length} 条行情`
    : '正在加载配置…';

  return (
    <main className="overlay" ref={overlayRef} style={appearance}>
      {!locked && <header className="overlay__toolbar">
        <div
          className="overlay__drag"
          onPointerDown={(event) => {
            if (event.button === 0) startDrag();
          }}
          title="拖动悬浮窗"
        >
          <span className="overlay__drag-icon" aria-hidden="true">⠿</span>
          <span className="overlay__title">StockOverlay</span>
        </div>
        <button className="overlay__settings" type="button" onClick={() => {
          invoke('open_settings').catch((reason: unknown) => {
            setWindowError(`打开设置失败：${errorMessage(reason)}`);
          });
        }}>设置</button>
        <span className="overlay__mode">编辑</span>
      </header>}
      <div className="overlay__status">{status}</div>
      {(windowError || error) && <div className="overlay__error" role="alert">{windowError || error}</div>}
      {config && <PaginatedStockList config={config} snapshot={snapshot} watchlist={watchlist} locked={locked} />}
      {!locked && <div
        className="overlay__resize"
        title="调整窗口大小"
        onPointerDown={(event) => {
          if (event.button === 0) startResize();
        }}
      />}
    </main>
  );
}
