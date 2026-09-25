import { invoke } from '@tauri-apps/api/core';
import { useEffect, useState } from 'react';
import { appStore } from '../appStore';
import type { AppConfig } from '../types/config';

export type ShortcutStatus = {
  lockRegistered: boolean;
  visibilityRegistered: boolean;
  lockError: string | null;
  visibilityError: string | null;
};

function errorMessage(error: unknown): string {
  if (typeof error === 'object' && error !== null && 'message' in error) return String(error.message);
  return String(error);
}

export function ShortcutSettings({
  config,
  status,
  onStatusChange,
}: {
  config: AppConfig;
  status: ShortcutStatus | null;
  onStatusChange: (status: ShortcutStatus) => void;
}) {
  const [lockKey, setLockKey] = useState(config.shortcuts.toggleLock);
  const [visibilityKey, setVisibilityKey] = useState(config.shortcuts.toggleVisibility);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    setLockKey(config.shortcuts.toggleLock);
    setVisibilityKey(config.shortcuts.toggleVisibility);
  }, [config.shortcuts.toggleLock, config.shortcuts.toggleVisibility]);

  const save = async (action: 'toggleLock' | 'toggleVisibility', key: string) => {
    try {
      const result = await invoke<ShortcutStatus>('change_shortcut', { action, key: key.trim() });
      onStatusChange(result);
      await appStore.refreshConfig();
      setMessage('快捷键已保存并生效');
    } catch (error) {
      await appStore.refreshConfig();
      setLockKey(config.shortcuts.toggleLock);
      setVisibilityKey(config.shortcuts.toggleVisibility);
      setMessage(`修改失败：${errorMessage(error)}`);
    }
  };

  return <div className="settings-form">
    <h2>全局快捷键</h2>
    <p className="settings-form__hint">请输入如 Ctrl+Alt+L 的组合；保存失败时原快捷键保持可用。</p>
    <form onSubmit={(event) => { event.preventDefault(); void save('toggleLock', lockKey); }}>
      <label className="settings-form__row">锁定 / 解锁
        <input value={lockKey} onChange={(event) => setLockKey(event.target.value)} />
      </label>
      <button type="submit">保存锁定快捷键</button>
      <span className="settings-form__hint">{status?.lockRegistered ? '已注册' : '未注册'}</span>
    </form>
    <form onSubmit={(event) => { event.preventDefault(); void save('toggleVisibility', visibilityKey); }}>
      <label className="settings-form__row">显示 / 隐藏
        <input value={visibilityKey} onChange={(event) => setVisibilityKey(event.target.value)} />
      </label>
      <button type="submit">保存显示快捷键</button>
      <span className="settings-form__hint">{status?.visibilityRegistered ? '已注册' : '未注册'}</span>
    </form>
    {message && <p role="status">{message}</p>}
  </div>;
}
