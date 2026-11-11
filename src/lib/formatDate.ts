/**
 * 后端时间戳的解析与展示。
 *
 * 新记录使用 UTC ISO 时间戳；旧记录仍可能是 `datetime('now', 'localtime')`
 * 写入的 `"2026-06-21 19:36:00"` —— 既不是 ISO 8601，也不带时区。
 * 对这种旧字符串直接调用 `new Date()` 的行为属于实现定义，Chromium 与
 * 实际发布用的 WKWebView 可能给出不同结果。
 *
 * 这里显式按本地时间解析旧格式，新标准格式则交给 ISO 解析，兼顾迁移前后。
 */

const LOCAL_DATETIME = /^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2}):(\d{2})$/;

/** 解析后端时间戳；无法识别时返回 `null` 而不是 `Invalid Date`。 */
export function parseTimestamp(value: string | null | undefined): Date | null {
  if (!value) return null;

  const trimmed = value.trim();
  const local = LOCAL_DATETIME.exec(trimmed);
  if (local) {
    const [, year, month, day, hour, minute, second] = local;
    // 后端存的就是本地时间，所以要用本地时间构造函数，不能走 Date.parse。
    return new Date(
      Number(year),
      Number(month) - 1,
      Number(day),
      Number(hour),
      Number(minute),
      Number(second)
    );
  }

  const parsed = new Date(trimmed);
  return Number.isNaN(parsed.getTime()) ? null : parsed;
}

/** 格式化为 `2026/6/21` 这样的本地日期。无法解析时返回空串，不显示 "Invalid Date"。 */
export function formatDate(value: string | null | undefined): string {
  const date = parseTimestamp(value);
  return date ? date.toLocaleDateString("zh-CN") : "";
}
