/** 开始一次新的请求，返回「这次请求是否仍是最新的」判定函数。 */
export type IsCurrentRun = () => boolean;
export type BeginRun = () => IsCurrentRun;

/**
 * 只保留最新一次异步结果的守卫。
 *
 * 应用里重叠的 `invoke()` 非常多 —— 侧边栏点一次分类最多会并发 5 个
 * `list_prompts`，而响应不保证按发出顺序返回。没有守卫时「最慢的赢」，
 * 界面会显示与当前筛选不匹配的数据（标题是全部 Prompt，内容却是收藏列表）。
 *
 * 用法：
 *   const isLatest = beginLoad();
 *   const data = await invoke(...);
 *   if (!isLatest()) return;   // 已有更新的请求，丢弃这次结果
 *   set({ data });
 *
 * 也可以只调用返回的 begin 而不取判定函数，用来「作废在途请求」。
 */
export function createLatestOnlyGuard(): BeginRun {
  let latest = 0;

  return function begin(): IsCurrentRun {
    latest += 1;
    const current = latest;
    return () => current === latest;
  };
}
