import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FolderOpen } from "lucide-react";

import { Checkbox } from "@/components/ui/checkbox";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { cn } from "@/lib/utils";

interface ScriptInfo {
  name: string | null;
  author: string | null;
  version: string | null;
  url: string | null;
  dependencies: string[];
  enabled: boolean;
  path: string;
}

interface ScriptsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function errMsg(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}

export function ScriptsDialog({ open, onOpenChange }: ScriptsDialogProps) {
  const [scripts, setScripts] = useState<ScriptInfo[] | null>(null);
  const [scriptsRoot, setScriptsRoot] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    try {
      setScripts(await invoke<ScriptInfo[]>("list_scripts"));
      setError(null);
    } catch (e) {
      setError(errMsg(e));
    }
    try {
      setScriptsRoot(await invoke<string>("scripts_dir"));
    } catch {
      // folder button stays inert
    }
  };

  useEffect(() => {
    if (open) void load();
  }, [open]);

  const toggle = async (script: ScriptInfo, enabled: boolean) => {
    // Optimistic update; revert if the backend rejects.
    setScripts((prev) =>
      prev ? prev.map((s) => (s.path === script.path ? { ...s, enabled } : s)) : prev,
    );
    try {
      await invoke("set_script_enabled", { path: script.path, enabled });
    } catch {
      setScripts((prev) =>
        prev
          ? prev.map((s) => (s.path === script.path ? { ...s, enabled: !enabled } : s))
          : prev,
      );
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Scripts</DialogTitle>
          <DialogDescription>
              {scripts ? `${scripts.length} script${scripts.length === 1 ? "" : "s"} found` : "loading…"}
          </DialogDescription>
        </DialogHeader>

        {error && <p className="text-xs text-destructive">{error}</p>}

        <ScrollArea className="h-80">
          <div className="flex flex-col gap-2 pr-3">
            {scripts === null ? (
              <p className="py-8 text-center text-xs text-muted-foreground">
                loading…
              </p>
            ) : scripts.length === 0 ? (
              <p className="py-8 text-center text-xs text-muted-foreground">
                no scripts found in ~/.lje/scripts
              </p>
            ) : (
              scripts.map((script) => (
                <div
                  key={script.path}
                  className="flex items-start justify-between gap-3 rounded-[10px] bg-[#252525] px-3.5 py-2.5"
                >
                  <div className="min-w-0">
                    <p className="truncate text-sm font-medium text-foreground">
                      {script.name ?? "unknown"}
                    </p>
                    <p className="text-xs text-muted-foreground">
                      {script.author ?? "unknown"}
                      {script.version ? ` · ${script.version}` : ""}
                    </p>
                    {script.url && (
                      <p className="truncate text-xs text-[#dda770]">
                        {script.url}
                      </p>
                    )}
                    {script.dependencies.length > 0 && (
                      <div className="mt-1 flex flex-wrap gap-1">
                        {script.dependencies.map((dep) => {
                          const depScript = scripts.find((s) => s.name === dep);
                          const isDisabled = depScript !== undefined && !depScript.enabled;
                          return (
                            <Badge
                              key={dep}
                              variant="secondary"
                              title={isDisabled ? `${dep} (disabled)` : dep}
                              className={cn(
                                "px-1.5 py-px text-[10px]",
                                isDisabled &&
                                  "border border-[#dda770]/30 bg-[#dda770]/10 text-[#dda770]",
                              )}
                            >
                              {dep}
                            </Badge>
                          );
                        })}
                      </div>
                    )}
                  </div>
                  <div className="flex shrink-0 items-center gap-1">
                    <button
                      type="button"
                              onClick={() =>
                        void invoke("open_folder", { path: script.path }).catch(console.error)
                      }
                      aria-label={`Open folder of ${script.name ?? script.path}`}
                      title="Open script folder"
                      className="cursor-pointer p-1 text-muted-foreground hover:text-foreground"
                    >
                      <FolderOpen className="size-3.5" />
                    </button>
                    <Checkbox
                      checked={script.enabled}
                      onCheckedChange={(checked) => void toggle(script, checked)}
                      aria-label={`toggle ${script.name ?? script.path}`}
                      className="shrink-0"
                    />
                  </div>
                </div>
              ))
            )}
          </div>
        </ScrollArea>

        <div className="flex justify-end">
          <button
            type="button"
            onClick={() =>
              scriptsRoot && void invoke("open_folder", { path: scriptsRoot }).catch(console.error)
            }
            className="cursor-pointer text-[11px] text-[#dda770] hover:underline"
          >
            open folder
          </button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
