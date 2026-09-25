import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useEffect, useRef, useState, type KeyboardEvent } from 'react';
import { appStore } from '../appStore';
import { shortcutFromKeyEvent } from '../shortcutCapture';
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
  const [recording, setRecording] = useState<'toggleLock' | 'toggleVisibility' | null>(null);
  const recordingRef = useRef<'toggleLock' | 'toggleVisibility' | null>(null);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    let stop: (() => void) | undefined;
    void listen<'toggleLock' | 'toggleVisibility'>('shortcut:pressed', (event) => {
      if (!active) return;
      const target = recordingRef.current;
      if (!target) return;
      recordingRef.current = null;
      setRecording(null);
      setMessage(target === event.payload
        ? '这是当前快捷键，未修改设置。'
        : '组合键已被另一个功能占用，原快捷键保持不变。');
    }).then((unlisten) => {
      if (active) stop = unlisten;
      else unlisten();
    }).catch((error: unknown) => {
      if (active) setMessage(`无法监听快捷键冲突：${errorMessage(error)}`);
    });
    return () => { active = false; stop?.(); };
  }, []);

  const save = async (action: 'toggleLock' | 'toggleVisibility', key: string) => {
    setSaving(true);
    try {
      const result = await invoke<ShortcutStatus>('change_shortcut', { action, key });
      onStatusChange(result);
      await appStore.refreshConfig();
      setMessage(`快捷键已保存并生效：${key}`);
    } catch (error) {
      await appStore.refreshConfig();
      setMessage(`修改失败：${errorMessage(error)}`);
    } finally {
      setSaving(false);
    }
  };

  const capture = (event: KeyboardEvent<HTMLButtonElement>, action: 'toggleLock' | 'toggleVisibility') => {
    if (recording !== action) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.repeat) return;
    if (event.key === 'Escape') {
      recordingRef.current = null;
      setRecording(null);
      setMessage('已取消录制');
      return;
    }
    const key = shortcutFromKeyEvent(event.nativeEvent);
    if (!key) {
      setMessage('请按住至少一个修饰键，再按字母、数字或 F1～F24。');
      return;
    }
    recordingRef.current = null;
    setRecording(null);
    setMessage(null);
    void save(action, key);
  };

  const shortcutButton = (action: 'toggleLock' | 'toggleVisibility', label: string, key: string, registered: boolean | undefined) =>
    <div className="settings-form__row" key={action}>
      <span>{label}</span>
      <button type="button" className="shortcut-capture" disabled={saving}
        aria-label={`录制${label}快捷键`} aria-pressed={recording === action}
        onClick={() => { recordingRef.current = action; setRecording(action); setMessage('请按组合键；按 Esc 取消。'); }}
        onKeyDown={(event) => capture(event, action)}
        onBlur={(event) => {
          if (event.relatedTarget && recordingRef.current === action) {
            recordingRef.current = null;
            setRecording(null);
          }
        }}
      >{recording === action ? '请按组合键…' : key}</button>
      <span className="settings-form__hint">{registered ? '已注册' : '未注册'}</span>
    </div>;

  return <div className="settings-form">
    <h2>全局快捷键</h2>
    <p className="settings-form__hint">点击当前组合键，再按新的组合键即可保存；按 Esc 取消。修改失败时原快捷键保持可用。</p>
    {shortcutButton('toggleLock', '锁定 / 解锁', config.shortcuts.toggleLock, status?.lockRegistered)}
    {shortcutButton('toggleVisibility', '显示 / 隐藏', config.shortcuts.toggleVisibility, status?.visibilityRegistered)}
    {message && <p role="status">{message}</p>}
  </div>;
}
