export interface AppConfig {
  schemaVersion: 1;
  window: WindowConfig;
  display: DisplayConfig;
  shortcuts: ShortcutConfig;
  /** Exchange-prefixed codes such as `sh600519`. */
  stocks: string[];
  refreshInterval: 1000 | 3000 | 5000 | 30000 | 60000 | 1800000 | 3600000;
}

export interface WindowConfig {
  /** Logical pixel coordinates. */
  x: number;
  y: number;
  width: number;
  height: number;
  locked: boolean;
  alwaysOnTop: boolean;
}

export interface DisplayConfig {
  fontSize: number;
  backgroundOpacity: number;
  textOpacity: number;
  showName: boolean;
  showCode: boolean;
  showPrice: boolean;
  showChange: boolean;
  showChangePercent: boolean;
  showTurnover: boolean;
  showHigh: boolean;
  showLow: boolean;
  showTurnoverRate: boolean;
  showPE: boolean;
  showVolumeRatio: boolean;
  showOrderRatio: boolean;
  orderBookDepth: 0 | 1 | 3 | 5;
  redForRise: boolean;
}

export interface ShortcutConfig {
  toggleLock: string;
  toggleVisibility: string;
}

/** Only changed fields are sent; Rust merges them with its current config. */
export interface ConfigPatch {
  window?: Partial<Omit<WindowConfig, 'locked'>>;
  display?: Partial<DisplayConfig>;
  refreshInterval?: AppConfig['refreshInterval'];
}
