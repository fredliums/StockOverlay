import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
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
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [snapshot, setSnapshot] = useState<QuoteSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);

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
      <div className="overlay__title">StockOverlay</div>
      <div className="overlay__status">{status}</div>
    </main>
  );
}
