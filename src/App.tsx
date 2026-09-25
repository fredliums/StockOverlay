import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useState, useSyncExternalStore, type CSSProperties } from 'react';
import { appStore } from './appStore';
import { StockList } from './components/StockList';

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

type ShortcutStatus = {
  lockError: string | null;
  visibilityError: string | null;
};

function SettingsShell() {
  const { config, snapshot, error } = useAppState();
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
      {shortcuts?.lockError && <p role="alert">锁定快捷键不可用：{shortcuts.lockError}</p>}
      {shortcuts?.visibilityError && <p role="alert">显示快捷键不可用：{shortcuts.visibilityError}</p>}
      {error && <p role="alert">{error}</p>}
    </main>
  );
}

function OverlayApp() {
  const { config, snapshot, watchlist, locked, error } = useAppState();
  const [windowError, setWindowError] = useState<string | null>(null);
  const appearance = config ? {
    '--quote-font-size': `${config.display.fontSize}px`,
    '--background-opacity': config.display.backgroundOpacity,
    '--text-opacity': config.display.textOpacity,
    '--rise-rgb': config.display.redForRise ? '255 105 98' : '74 207 142',
    '--fall-rgb': config.display.redForRise ? '74 207 142' : '255 105 98',
  } as CSSProperties : undefined;

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
    <main className="overlay" style={appearance}>
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
        <span className="overlay__mode">编辑</span>
      </header>}
      <div className="overlay__status">{status}</div>
      {(windowError || error) && <div className="overlay__error" role="alert">{windowError || error}</div>}
      {config && <StockList config={config} snapshot={snapshot} watchlist={watchlist} />}
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
