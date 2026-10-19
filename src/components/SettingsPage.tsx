import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { useSettingsStore } from "@/stores/settingsStore";
import { useToast } from "@/components/ui/toast";
import { cn, getErrorMessage } from "@/lib/utils";
import { DeleteConfirmModal } from "@/components/DeleteConfirmModal";
import { formatDate } from "@/lib/formatDate";

const PROVIDERS: { value: string; label: string }[] = [
  { value: "openai", label: "OpenAI" },
  { value: "anthropic", label: "Anthropic" },
  { value: "deepseek", label: "DeepSeek" },
  { value: "glm", label: "GLM (智谱)" },
  { value: "minimax", label: "MiniMax" },
  { value: "kimi", label: "Kimi (月之暗面)" },
  { value: "other", label: "其他..." },
];

const DEFAULT_URLS: Record<string, string> = {
  openai: "https://api.openai.com/v1",
  anthropic: "https://api.anthropic.com",
  deepseek: "https://api.deepseek.com/v1",
  glm: "https://open.bigmodel.cn/api/paas/v4",
  minimax: "https://api.minimax.chat/v1",
  kimi: "https://api.moonshot.cn/v1",
};

interface FormState {
  name: string;
  provider: string;
  customProvider: string;
  baseUrl: string;
  apiKey: string;
  model: string;
}

const EMPTY_FORM: FormState = {
  name: "",
  provider: "openai",
  customProvider: "",
  baseUrl: "https://api.openai.com/v1",
  apiKey: "",
  model: "",
};

export function SettingsPage() {
  const configs = useSettingsStore((s) => s.configs);
  const configError = useSettingsStore((s) => s.error);
  const loadConfigs = useSettingsStore((s) => s.loadConfigs);
  const saveConfig = useSettingsStore((s) => s.saveConfig);
  const updateConfig = useSettingsStore((s) => s.updateConfig);
  const deleteConfig = useSettingsStore((s) => s.deleteConfig);
  const activateConfig = useSettingsStore((s) => s.activateConfig);
  const { addToast } = useToast();

  const [showForm, setShowForm] = useState(false);
  const [editId, setEditId] = useState<string | null>(null);
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const [isSaving, setIsSaving] = useState(false);
  const [isTesting, setIsTesting] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<{ id: string; name: string } | null>(null);

  useEffect(() => {
    loadConfigs();
  }, [loadConfigs]);

  const resetForm = () => {
    setForm(EMPTY_FORM);
    setEditId(null);
    setShowForm(false);
  };

  const handleProviderChange = (value: string | null) => {
    if (!value) return;
    if (value === "other") {
      setForm((f) => ({ ...f, provider: value, baseUrl: "", customProvider: "" }));
    } else {
      setForm((f) => ({ ...f, provider: value, baseUrl: DEFAULT_URLS[value] || "", customProvider: "" }));
    }
  };

  const handleSave = async () => {
    if (!form.name.trim() || !form.baseUrl.trim() || !form.model.trim()) {
      addToast("请填写名称、Base URL 和模型名称", "error");
      return;
    }
    if (form.provider === "other" && !form.customProvider.trim()) {
      addToast("请填写自定义 Provider 名称", "error");
      return;
    }
    if (!editId && !form.apiKey.trim()) {
      addToast("请填写 API Key", "error");
      return;
    }
    const provider = form.provider === "other" ? form.customProvider.trim() : form.provider;
    setIsSaving(true);
    try {
      if (editId) {
        await updateConfig(editId, form.name, provider, form.baseUrl, form.apiKey, form.model);
        addToast("配置已更新", "success");
      } else {
        await saveConfig(form.name, provider, form.baseUrl, form.apiKey, form.model);
        addToast("配置已保存", "success");
      }
      resetForm();
    } catch (e) {
      addToast(getErrorMessage(e), "error");
    } finally {
      setIsSaving(false);
    }
  };

  const handleEdit = (config: (typeof configs)[0]) => {
    const isBuiltin = PROVIDERS.some((p) => p.value === config.provider);
    setForm({
      name: config.name,
      provider: isBuiltin ? config.provider : "other",
      customProvider: isBuiltin ? "" : config.provider,
      baseUrl: config.base_url,
      apiKey: "",
      model: config.model,
    });
    setEditId(config.id);
    setShowForm(true);
  };

  const handleDelete = async (id: string, name: string) => {
    setDeleteTarget({ id, name });
  };

  const handleDeleteConfirm = async () => {
    if (!deleteTarget) return;
    try {
      await deleteConfig(deleteTarget.id);
      addToast("配置已删除", "info");
    } catch (e) {
      addToast(getErrorMessage(e), "error");
    } finally {
      setDeleteTarget(null);
    }
  };

  const handleActivate = async (id: string, name: string) => {
    try {
      await activateConfig(id);
      addToast(`${name} 已激活`, "success");
    } catch (e) {
      addToast(getErrorMessage(e), "error");
    }
  };

  const handleTestConnection = async () => {
    if (!form.baseUrl.trim() || !form.model.trim()) {
      addToast("请先填写 Base URL 和模型名称", "error");
      return;
    }
    if (!form.apiKey.trim()) {
      addToast("请先输入 API Key", "error");
      return;
    }
    if (form.provider === "other" && !form.customProvider.trim()) {
      addToast("请填写自定义 Provider 名称", "error");
      return;
    }
    const provider = form.provider === "other" ? form.customProvider.trim() : form.provider;
    setIsTesting(true);
    try {
      const result = await invoke<string>("test_connection", {
        provider,
        baseUrl: form.baseUrl,
        apiKey: form.apiKey,
        model: form.model,
      });
      addToast(result, "success");
    } catch (e) {
      addToast(getErrorMessage(e), "error");
    } finally {
      setIsTesting(false);
    }
  };

  return (
    <div className="flex-1 overflow-y-auto">
      <div className="pt-6 px-5">
        {/* Header */}
        <div className="flex items-center gap-3 mb-6">
          <div>
            <h1 className="text-[17px] font-semibold text-slate-100">模型 API 设置</h1>
            <p className="text-[12px] text-slate-500 mt-0.5">
              管理你的 AI 模型配置，支持多 API 切换
            </p>
          </div>
          <div className="flex-1" />
          <Button onClick={() => { resetForm(); setShowForm(true); }} size="sm" className="h-8 text-[12px]">
            添加配置
          </Button>
        </div>

        {/* 读取失败不能伪装成「暂无配置」 */}
        {configError && (
          <div
            role="alert"
            className="flex items-center gap-3 mb-4 px-3.5 py-2.5 rounded-[10px] border border-red-500/25 bg-red-500/[0.08]"
          >
            <span className="flex-1 min-w-0 text-[12px] text-red-200 break-words">
              读取 API 配置失败：{configError}
            </span>
            <Button
              size="sm"
              variant="outline"
              onClick={() => loadConfigs()}
              className="h-7 text-[11px] shrink-0"
            >
              重试
            </Button>
          </div>
        )}

        {/* Config list */}
        {configs.length === 0 && !showForm && !configError && (
          <Card className="border-dashed border-white/[0.06] bg-white/[0.02]">
            <CardContent className="flex flex-col items-center justify-center py-16 gap-3">
              <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor"
                   strokeWidth="1.5" strokeLinecap="round" className="text-slate-700">
                <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
                <circle cx="12" cy="12" r="3" />
              </svg>
              <p className="text-[12px] text-slate-500">暂无 API 配置</p>
              <Button onClick={() => setShowForm(true)} size="sm" variant="outline" className="h-8 text-[12px]">
                添加第一个配置
              </Button>
            </CardContent>
          </Card>
        )}

        <div className="space-y-3">
          {configs.map((config) => (
            <Card
              key={config.id}
              className={cn(
                "transition-colors duration-150 bg-white/[0.02] border-white/[0.06]",
                config.is_active && "ring-1 ring-purple-500/30 border-purple-500/30 bg-purple-500/[0.03]"
              )}
            >
              <CardHeader className="px-4 py-3 pb-2">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2.5">
                    <CardTitle className="text-[13px] font-medium text-slate-100">{config.name}</CardTitle>
                    {config.is_active && (
                      <span className="inline-flex items-center px-2 py-0.5 rounded-full text-[11px] font-medium bg-purple-500/15 text-purple-300">
                        使用中
                      </span>
                    )}
                  </div>
                  <span className="text-[11px] text-slate-600 tabular-nums">
                    {formatDate(config.created_at)}
                  </span>
                </div>
                <CardDescription className="text-[12px] mt-0.5 text-slate-500">
                  {config.provider} / {config.model}
                </CardDescription>
              </CardHeader>
              <CardContent className="px-4 py-0 pb-3">
                <div className="flex items-center gap-1.5">
                  {!config.is_active && (
                    <Button
                      size="sm"
                      variant="default"
                      className="h-7 text-[12px]"
                      onClick={() => handleActivate(config.id, config.name)}
                    >
                      启动
                    </Button>
                  )}
                  {config.is_active && (
                    <span className="text-[11px] text-slate-500 px-1">
                      当前激活的 API
                    </span>
                  )}
                  <div className="flex-1" />
                  <Button
                    size="sm"
                    variant="outline"
                    className="h-7 text-[12px]"
                    onClick={() => handleEdit(config)}
                  >
                    编辑
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    className="h-7 text-[12px] text-red-400 hover:text-red-300"
                    onClick={() => handleDelete(config.id, config.name)}
                  >
                    删除
                  </Button>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>

        {/* Add/Edit form */}
        {showForm && (
          <Card className="mt-4 bg-white/[0.02] border-white/[0.06]">
            <CardHeader className="px-4 py-3">
              <CardTitle className="text-[13px] text-slate-100">{editId ? "编辑配置" : "添加新配置"}</CardTitle>
            </CardHeader>
            <CardContent className="px-4 pb-4 space-y-2.5">
              <div className="space-y-1.5">
                <label className="text-[12px] font-medium text-slate-400">配置名称 *</label>
                <Input
                  value={form.name}
                  onChange={(e) => setForm((f) => ({ ...f, name: e.target.value }))}
                  placeholder="例如：我的 GPT-4.1"
                  className="h-8 text-[13px]"
                />
              </div>

              <div className="space-y-1.5">
                <label className="text-[12px] font-medium text-slate-400">Provider</label>
                <Select value={form.provider} onValueChange={handleProviderChange}>
                  <SelectTrigger className="h-8 text-[12px]">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {PROVIDERS.map((p) => (
                      <SelectItem key={p.value} value={p.value}>
                        {p.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>

              {form.provider === "other" && (
                <div className="space-y-1.5">
                  <label className="text-[12px] font-medium text-slate-400">自定义 Provider 名称 *</label>
                  <Input
                    value={form.customProvider}
                    onChange={(e) => setForm((f) => ({ ...f, customProvider: e.target.value }))}
                    placeholder="例如：ollama"
                    className="h-8 text-[13px]"
                  />
                </div>
              )}

              <div className="space-y-1.5">
                <label className="text-[12px] font-medium text-slate-400">Base URL *</label>
                <Input
                  value={form.baseUrl}
                  onChange={(e) => setForm((f) => ({ ...f, baseUrl: e.target.value }))}
                  placeholder="https://api.openai.com/v1"
                  className="h-8 text-[13px]"
                />
              </div>

              <div className="space-y-1.5">
                <label className="text-[12px] font-medium text-slate-400">API Key</label>
                <Input
                  type="password"
                  value={form.apiKey}
                  onChange={(e) => setForm((f) => ({ ...f, apiKey: e.target.value }))}
                  placeholder={editId ? "留空则不修改（修改主机后需重填）" : "sk-xxxx"}
                  className="h-8 text-[13px]"
                />
                <p className="text-[11px] text-slate-600">
                  API Key 仅保存在本机应用数据中，不会上传到 PromptDock
                </p>
              </div>

              <div className="space-y-1.5">
                <label className="text-[12px] font-medium text-slate-400">Model Name *</label>
                <Input
                  value={form.model}
                  onChange={(e) => setForm((f) => ({ ...f, model: e.target.value }))}
                  placeholder="gpt-4.1-mini"
                  className="h-8 text-[13px]"
                />
              </div>

              <div className="flex gap-1.5 pt-1">
                <Button onClick={handleSave} disabled={isSaving} size="sm" className="h-8 text-[12px]">
                  {isSaving ? "保存中..." : editId ? "更新" : "保存"}
                </Button>
                <Button
                  onClick={handleTestConnection}
                  disabled={isTesting || !form.apiKey.trim()}
                  size="sm"
                  variant="outline"
                  className="h-8 text-[12px]"
                >
                  {isTesting ? "测试中..." : "测试连接"}
                </Button>
                <Button onClick={resetForm} size="sm" variant="ghost" className="h-8 text-[12px]">
                  取消
                </Button>
              </div>
            </CardContent>
          </Card>
        )}
      </div>

      <DeleteConfirmModal
        open={deleteTarget !== null}
        title={deleteTarget?.name ?? ""}
        onConfirm={handleDeleteConfirm}
        onCancel={() => setDeleteTarget(null)}
      />
    </div>
  );
}
