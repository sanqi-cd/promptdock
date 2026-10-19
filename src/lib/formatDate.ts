/**
 * 后端时间戳的解析与展示。
 *
 * 后端用 `datetime('now', 'localtime')` 写入，得到的是 `"2026-06-21 19:36:00"`
 * —— 既不是 ISO 8601，也不带时区。对这种字符串调用 `new Date()` 的行为是
 * **ECMA-262 未规定的**（非 ISO 格式交给各引擎自行实现），所以跑 e2e 的
 * Chromium 和实际发布用的 WKWebView 可以给出不同结果；而且缺少时区标记时，
 * 各家是按 UTC 还是本地时间解释也不一致。
 *
 * 这里显式解析，两个问题一起消掉。
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
