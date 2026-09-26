import { useState } from 'react';
import { appStore } from '../appStore';
import type { AppConfig, ConfigPatch, DisplayConfig } from '../types/config';

const fieldOptions = [
  { key: 'showName', label: '股票名称' },
  { key: 'showCode', label: '股票代码' },
  { key: 'showPrice', label: '最新价格' },
  { key: 'showChange', label: '涨跌额' },
  { key: 'showChangePercent', label: '涨跌幅' },
  { key: 'showTurnover', label: '成交额' },
  { key: 'showHigh', label: '最高价' },
  { key: 'showLow', label: '最低价' },
  { key: 'showTurnoverRate', label: '换手率' },
  { key: 'showPE', label: '市盈率 PE TTM' },
  { key: 'showVolumeRatio', label: '量比' },
  { key: 'showOrderRatio', label: '委比' },
] as const;

function errorMessage(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error) return String(error.message);
  return String(error);
}

function useSaveSetting() {
  const [status, setStatus] = useState<string | null>(null);
  const save = async (patch: ConfigPatch) => {
    try {
      await appStore.saveConfig(patch);
      setStatus('已保存');
    } catch (error) {
      setStatus(`保存失败：${errorMessage(error)}`);
    }
  };
  return { status, save };
}

export function DisplaySettings({ config }: { config: AppConfig }) {
  const { status, save } = useSaveSetting();
  return <div className="settings-form">
    <h2>显示字段</h2>
    <div className="settings-form__checks">
      {fieldOptions.map((field) => <label key={field.key}>
        <input type="checkbox" checked={config.display[field.key]} onChange={(event) => {
          void save({ display: { [field.key]: event.target.checked } as Partial<DisplayConfig> });
        }} />
        {field.label}
      </label>)}
    </div>
    <label className="settings-form__row">盘口档位
      <select value={config.display.orderBookDepth} onChange={(event) => {
        void save({ display: { orderBookDepth: Number(event.target.value) as DisplayConfig['orderBookDepth'] } });
      }}>
        <option value={0}>不显示</option>
        <option value={1}>一档</option>
        <option value={3}>三档</option>
        <option value={5}>五档</option>
      </select>
    </label>
    <p className="settings-form__hint">盘口挂单量单位：手。一档按“卖一价格-量 / 买一价格-量”固定顺序显示。</p>
    {status && <p role="status">{status}</p>}
  </div>;
}

export function AppearanceSettings({ config }: { config: AppConfig }) {
  const { status, save } = useSaveSetting();
  return <div className="settings-form">
    <h2>外观</h2>
    <label className="settings-form__row">字体大小：{config.display.fontSize} px
      <select value={config.display.fontSize} onChange={(event) => {
        void save({ display: { fontSize: Number(event.target.value) } });
      }}>
        {Array.from({ length: 39 }, (_, index) => index + 10).map((size) => <option key={size} value={size}>{size} px</option>)}
      </select>
    </label>
    <label className="settings-form__row">背景不透明度：{Math.round(config.display.backgroundOpacity * 100)}%
      <select value={config.display.backgroundOpacity} onChange={(event) => {
        void save({ display: { backgroundOpacity: Number(event.target.value) } });
      }}>
        {Array.from({ length: 21 }, (_, index) => index * 5).map((percent) =>
          <option key={percent} value={percent / 100}>{percent}%</option>)}
      </select>
    </label>
    <label className="settings-form__row">文字不透明度：{Math.round(config.display.textOpacity * 100)}%
      <select value={config.display.textOpacity} onChange={(event) => {
        void save({ display: { textOpacity: Number(event.target.value) } });
      }}>
        {Array.from({ length: 21 }, (_, index) => index * 5).map((percent) =>
          <option key={percent} value={percent / 100}>{percent}%</option>)}
      </select>
    </label>
    <label className="settings-form__row">涨跌配色
      <select value={config.display.redForRise ? 'red' : 'green'} onChange={(event) => {
        void save({ display: { redForRise: event.target.value === 'red' } });
      }}>
        <option value="red">红涨绿跌</option>
        <option value="green">绿涨红跌</option>
      </select>
    </label>
    {status && <p role="status">{status}</p>}
  </div>;
}

export function QuoteSettings({ config }: { config: AppConfig }) {
  const { status, save } = useSaveSetting();
  return <div className="settings-form">
    <h2>行情</h2>
    <label className="settings-form__row">刷新间隔
      <select value={config.refreshInterval} onChange={(event) => {
        void save({ refreshInterval: Number(event.target.value) as AppConfig['refreshInterval'] });
      }}>
        <option value={1000}>1 秒</option>
        <option value={3000}>3 秒</option>
        <option value={5000}>5 秒</option>
        <option value={30000}>30 秒</option>
        <option value={60000}>60 秒</option>
        <option value={1800000}>30 分钟</option>
        <option value={3600000}>60 分钟</option>
      </select>
    </label>
    {status && <p role="status">{status}</p>}
  </div>;
}
