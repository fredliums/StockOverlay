import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import type { AppConfig } from './types/config';
import type { QuoteSnapshot } from './types/quote';

function errorMessage(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error) {
    return String(error.message);
  }
  return String(error);
}

export default function App() {
  const [status, setStatus] = useState('正在加载配置…');

  useEffect(() => {
    let active = true;
    Promise.all([
      invoke<AppConfig>('load_config'),
      invoke<QuoteSnapshot>('get_quote_snapshot'),
    ])
      .then(([config, snapshot]) => {
        if (active) {
          setStatus(`${config.stocks.length} 只自选股 · ${snapshot.quotes.length} 条行情`);
        }
      })
      .catch((error: unknown) => {
        if (active) {
          setStatus(`加载失败：${errorMessage(error)}`);
        }
      });
    return () => {
      active = false;
    };
  }, []);

  return (
    <main className="overlay">
      <div className="overlay__title">StockOverlay</div>
      <div className="overlay__status">{status}</div>
    </main>
  );
}
