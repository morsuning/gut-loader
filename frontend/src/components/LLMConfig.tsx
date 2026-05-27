import { useEffect, useRef, useState } from "react";
import { Check, Eye, EyeOff, Loader2, Sparkles, Wand2 } from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useAppStore } from "@/stores/appStore";
import { useTauriCommands } from "@/hooks/useTauriCommands";

/**
 * LLM 配置块：API URL / API Key / 模型名称，并提供测试连接。
 * 既可独立使用，也可嵌入数据库配置面板。
 */
export function LLMConfig() {
  const llmConfig = useAppStore((s) => s.llmConfig);
  const setLlmConfig = useAppStore((s) => s.setLlmConfig);
  const { testLlmConnection } = useTauriCommands();

  const [showKey, setShowKey] = useState(false);
  const [testing, setTesting] = useState(false);
  const [saved, setSaved] = useState(false);
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 配置变更时显示"已保存"反馈
  const isFirstRender = useRef(true);
  useEffect(() => {
    if (isFirstRender.current) {
      isFirstRender.current = false;
      return;
    }
    setSaved(true);
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => setSaved(false), 2000);
    return () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
    };
  }, [llmConfig.api_url, llmConfig.api_key, llmConfig.model]);

  const handleTest = async () => {
    if (!llmConfig.api_key) {
      toast.warning("请填写 API Key 后再测试");
      return;
    }
    setTesting(true);
    try {
      const ok = await testLlmConnection(llmConfig);
      if (ok) {
        toast.success("LLM 连接可用");
      } else {
        toast.error("LLM 连接失败", {
          description: "请检查 API URL / Key / 模型 是否正确",
        });
      }
    } finally {
      setTesting(false);
    }
  };

  return (
    /* DESIGN.md: LLM 配置区域用奶油色 muted 背景 */
    <div className="space-y-4 rounded-lg border bg-muted/30 p-4">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          {/* DESIGN.md: LLM 图标用 accent（琥珀色） */}
          <Sparkles className="h-4 w-4 text-accent" />
          <p className="text-xs font-mono uppercase tracking-[0.18em] text-muted-foreground">
            LLM Endpoint
          </p>
        </div>
        {saved && (
          /* DESIGN.md: 保存成功用 success 绿色 */
          <span className="flex items-center gap-1 text-xs text-success animate-in fade-in">
            <Check className="h-3 w-3" />
            已自动保存
          </span>
        )}
      </div>

      <div className="grid gap-3 md:grid-cols-2">
        <div className="space-y-1">
          <Label htmlFor="llm-url">API URL</Label>
          <Input
            id="llm-url"
            value={llmConfig.api_url}
            onChange={(e) => setLlmConfig({ api_url: e.target.value })}
            placeholder="https://api.openai.com/v1"
            className="font-mono text-sm"
          />
        </div>
        <div className="space-y-1">
          <Label htmlFor="llm-model">模型</Label>
          <Input
            id="llm-model"
            value={llmConfig.model}
            onChange={(e) => setLlmConfig({ model: e.target.value })}
            placeholder="gpt-4o-mini"
            className="font-mono text-sm"
          />
        </div>
      </div>

      <div className="space-y-1">
        <Label htmlFor="llm-key">API Key</Label>
        <div className="flex gap-2">
          <div className="relative flex-1">
            <Input
              id="llm-key"
              type={showKey ? "text" : "password"}
              value={llmConfig.api_key}
              onChange={(e) => setLlmConfig({ api_key: e.target.value })}
              placeholder="sk-..."
              className="pr-10 font-mono text-sm"
            />
            <button
              type="button"
              onClick={() => setShowKey((s) => !s)}
              className="absolute inset-y-0 right-0 grid w-10 place-items-center text-muted-foreground hover:text-foreground"
              aria-label={showKey ? "隐藏" : "显示"}
            >
              {showKey ? (
                <EyeOff className="h-4 w-4" />
              ) : (
                <Eye className="h-4 w-4" />
              )}
            </button>
          </div>
          <Button
            type="button"
            variant="outline"
            onClick={handleTest}
            disabled={testing}
            className="gap-2"
          >
            {testing ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Wand2 className="h-4 w-4" />
            )}
            测试连接
          </Button>
        </div>
      </div>
    </div>
  );
}
