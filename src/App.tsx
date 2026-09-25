import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { LogicalSize } from '@tauri-apps/api/dpi';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useState } from 'react';
import type { AppConfig } from './types/config';
import type { QuoteSnapshot } from './types/quote';
import { acceptNewerSnapshot } from './snapshot';

function errorMessage(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error) {
    return String(error.message);
  }
  return String(error);
}

export default function App() {
  if (getCurrentWindow().label === 'settings') {
    return <SettingsShell />;
  }
  return <OverlayApp />;
}

type ShortcutStatus = {
  lockError: string | null;
  visibilityError: string | null;
};

function SettingsShell() {
  const [status, setStatus] = useState<ShortcutStatus | null>(null);
  useEffect(() => {
    invoke<ShortcutStatus>('get_shortcut_status').then(setStatus).catch(() => {});
  }, []);
  return (
    <main className="settings-shell">
      <h1>StockOverlay 设置</h1>
      {status?.lockError && <p role="alert">锁定快捷键不可用：{status.lockError}</p>}
      {status?.visibilityError && <p role="alert">显示快捷键不可用：{status.visibilityError}</p>}
    </main>
  );
}

function OverlayApp() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [snapshot, setSnapshot] = useState<QuoteSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [windowError, setWindowError] = useState<string | null>(null);
  const [locked, setLocked] = useState(false);

  useEffect(() => {
    let active = true;
    let stopMode: (() => void) | undefined;
    let stopError: (() => void) | undefined;
    listen<boolean>('window:mode', (event) => {
      if (active) setLocked(event.payload);
    }).then((stop) => {
      if (!active) {
        stop();
        return;
      }
      stopMode = stop;
      invoke<boolean>('get_window_mode')
        .then((mode) => { if (active) setLocked(mode); })
        .catch((reason: unknown) => { if (active) setWindowError(errorMessage(reason)); });
    }).catch((reason: unknown) => { if (active) setWindowError(errorMessage(reason)); });
    listen<string>('window:error', (event) => {
      if (active) setWindowError(event.payload);
    }).then((stop) => { if (active) stopError = stop; else stop(); });
    return () => {
      active = false;
      stopMode?.();
      stopError?.();
    };
  }, []);

  useEffect(() => {
    let active = true;
    getCurrentWindow()
      .setMinSize(new LogicalSize(240, 80))
      .catch((reason: unknown) => {
        if (active) setWindowError(`窗口最小尺寸设置失败：${errorMessage(reason)}`);
      });
    return () => {
      active = false;
    };
  }, []);

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

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    const acceptSnapshot = (next: QuoteSnapshot) => {
      if (active) {
        setSnapshot((current) => acceptNewerSnapshot(current, next));
      }
    };
    const readSnapshot = () => {
      invoke<QuoteSnapshot>('get_quote_snapshot')
        .then(acceptSnapshot)
        .catch((reason: unknown) => {
          if (active) setError(errorMessage(reason));
        });
    };
    const onVisible = () => {
      if (document.visibilityState === 'visible') readSnapshot();
    };

    listen<QuoteSnapshot>('quote:update', (event) => acceptSnapshot(event.payload))
      .then((stop) => {
        if (!active) {
          stop();
          return;
        }
        unlisten = stop;
        document.addEventListener('visibilitychange', onVisible);
        window.addEventListener('focus', readSnapshot);
        readSnapshot();
      })
      .catch((reason: unknown) => {
        if (active) setError(errorMessage(reason));
      });
    invoke<AppConfig>('load_config')
      .then((loaded) => {
        if (active) setConfig(loaded);
      })
      .catch((reason: unknown) => {
        if (active) setError(errorMessage(reason));
      });
    return () => {
      active = false;
      unlisten?.();
      document.removeEventListener('visibilitychange', onVisible);
      window.removeEventListener('focus', readSnapshot);
    };
  }, []);

  const status = error
    ? `加载失败：${error}`
    : config && snapshot
      ? `${config.stocks.length} 只自选股 · ${snapshot.quotes.length} 条行情`
      : '正在加载配置…';

  return (
    <main className="overlay">
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
      {windowError && <div className="overlay__error" role="alert">{windowError}</div>}
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
