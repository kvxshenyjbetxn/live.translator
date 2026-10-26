type Key = Pick<KeyboardEvent, 'code' | 'ctrlKey' | 'metaKey' | 'altKey' | 'shiftKey' | 'repeat'>;
export function shortcutFromKey(event: Key): string | null {
  if (event.repeat || !(event.ctrlKey || event.metaKey || event.altKey || event.shiftKey)) return null;
  if (!/^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-9]|2[0-4])|Space|Enter|Tab|Backspace|Delete|Insert|Home|End|PageUp|PageDown|Arrow(Up|Down|Left|Right)|Minus|Equal|BracketLeft|BracketRight|Backslash|Semicolon|Quote|Comma|Period|Slash|Backquote)$/.test(event.code)) return null;
  const parts: string[] = [];
  if (event.metaKey) parts.push('Super');
  if (event.ctrlKey) parts.push('Control');
  if (event.altKey) parts.push('Alt');
  if (event.shiftKey) parts.push('Shift');
  return [...parts, event.code].join('+');
}
