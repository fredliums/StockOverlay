import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { acceptNewerSnapshot } from './snapshot';
import type { AppConfig, ConfigPatch } from './types/config';
import type { QuoteSnapshot } from './types/quote';
import type { StockEntry } from './types/stock';

export type AppState = {
  config: AppConfig | null;
  snapshot: QuoteSnapshot | null;
  watchlist: StockEntry[];
  locked: boolean;
  visible: boolean;
  error: string | null;
};

function errorMessage(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error) {
    return String(error.message);
  }
  return String(error);
}

class AppStore {
  private state: AppState = {
    config: null,
    snapshot: null,
    watchlist: [],
    locked: false,
    visible: document.visibilityState === 'visible',
    error: null,
  };
  private listeners = new Set<() => void>();
  private connections = 0;
  private token = 0;
  private stops: UnlistenFn[] = [];
  private configRead = 0;
  private modeRead = 0;
  private watchlistRead = 0;

  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  };

  getState = () => this.state;

  private update(patch: Partial<AppState>) {
    this.state = { ...this.state, ...patch };
    this.listeners.forEach((listener) => listener());
  }

  connect = () => {
    this.connections += 1;
    if (this.connections === 1) void this.start();
    return () => {
      this.connections -= 1;
      if (this.connections === 0) this.stop();
    };
  };

  private async start() {
    const token = ++this.token;
    const subscriptions = await Promise.allSettled([
      listen<QuoteSnapshot>('quote:update', (event) => {
        const snapshot = acceptNewerSnapshot(this.state.snapshot, event.payload);
        if (snapshot !== this.state.snapshot) this.update({ snapshot });
      }),
      listen<AppConfig>('config:update', () => { void this.refreshConfig(); }),
      listen<boolean>('window:mode', (event) => {
        this.modeRead += 1;
        this.update({ locked: event.payload });
      }),
      listen<string>('window:error', (event) => this.update({ error: event.payload })),
    ]);
    const stops = subscriptions.flatMap((result) => result.status === 'fulfilled' ? [result.value] : []);
    if (token !== this.token) {
      stops.forEach((stop) => stop());
      return;
    }
    if (subscriptions.some((result) => result.status === 'rejected')) {
      stops.forEach((stop) => stop());
      this.update({ error: '无法订阅应用状态更新。' });
      return;
    }
    this.stops = stops;
    document.addEventListener('visibilitychange', this.onVisible);
    window.addEventListener('focus', this.onFocus);
    this.refresh();
  }

  private stop() {
    this.token += 1;
    this.stops.forEach((stop) => stop());
    this.stops = [];
    document.removeEventListener('visibilitychange', this.onVisible);
    window.removeEventListener('focus', this.onFocus);
  }

  private onVisible = () => {
    const visible = document.visibilityState === 'visible';
    this.update({ visible });
    if (visible) this.refresh();
  };

  private onFocus = () => { this.refresh(); };

  refresh = () => {
    void this.refreshConfig();
    void this.refreshSnapshot();
    void this.refreshMode();
  };

  async refreshConfig() {
    const read = ++this.configRead;
    try {
      const config = await invoke<AppConfig>('load_config');
      if (read === this.configRead && this.connections > 0) {
        this.update({ config });
        void this.refreshWatchlist();
      }
    } catch (error) {
      if (read === this.configRead && this.connections > 0) {
        this.update({ error: errorMessage(error) });
      }
    }
  }

  async refreshSnapshot() {
    try {
      const snapshot = await invoke<QuoteSnapshot>('get_quote_snapshot');
      if (this.connections > 0) {
        const next = acceptNewerSnapshot(this.state.snapshot, snapshot);
        if (next !== this.state.snapshot) this.update({ snapshot: next });
      }
    } catch (error) {
      if (this.connections > 0) this.update({ error: errorMessage(error) });
    }
  }

  async refreshWatchlist() {
    const read = ++this.watchlistRead;
    try {
      const watchlist = await invoke<StockEntry[]>('get_watchlist_entries');
      if (read === this.watchlistRead && this.connections > 0) {
        this.update({ watchlist });
      }
    } catch (error) {
      if (read === this.watchlistRead && this.connections > 0) {
        this.update({ error: errorMessage(error) });
      }
    }
  }

  async refreshMode() {
    const read = ++this.modeRead;
    try {
      const locked = await invoke<boolean>('get_window_mode');
      if (read === this.modeRead && this.connections > 0) this.update({ locked });
    } catch (error) {
      if (read === this.modeRead && this.connections > 0) {
        this.update({ error: errorMessage(error) });
      }
    }
  }

  private async command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
    try {
      const result = await invoke<T>(name, args);
      this.update({ error: null });
      return result;
    } catch (error) {
      this.update({ error: errorMessage(error) });
      throw error;
    }
  }

  async saveConfig(patch: ConfigPatch) {
    const saved = await this.command<AppConfig>('save_config', { patch });
    await this.refreshConfig();
    return saved;
  }

  async addStock(code: string) {
    const saved = await this.command<AppConfig>('add_stock', { code });
    await this.refreshConfig();
    return saved;
  }

  async removeStock(code: string) {
    const saved = await this.command<AppConfig>('remove_stock', { code });
    await this.refreshConfig();
    return saved;
  }

  async reorderStocks(codes: string[]) {
    const saved = await this.command<AppConfig>('reorder_stocks', { codes });
    await this.refreshConfig();
    return saved;
  }

  searchStocks(query: string) {
    return this.command<StockEntry[]>('search_stocks', { query });
  }
}

export const appStore = new AppStore();
