import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";

interface Settings {
  launchArgs: string;
  releaseBranch: string;
}

interface SettingsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function errMsg(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}

export function SettingsDialog({ open, onOpenChange }: SettingsDialogProps) {
  const [launchArgs, setLaunchArgs] = useState("-console");
  const [releaseBranch, setReleaseBranch] = useState("expansion");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setError(null);
    void (async () => {
      try {
        const settings = await invoke<Settings>("settings_get");
        setLaunchArgs(settings.launchArgs);
        setReleaseBranch(settings.releaseBranch);
      } catch {
        // keep defaults
      }
    })();
  }, [open]);

  const save = async () => {
    try {
      await invoke("settings_save", { launchArgs, releaseBranch });
      onOpenChange(false);
    } catch (e) {
      setError(errMsg(e));
    }
  };

  const inputClass =
    "w-full rounded-[10px] bg-[#252525] px-3.5 py-2.5 font-mono text-xs text-[#888] caret-[#888] outline-none";

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>Settings</DialogTitle>
          <DialogDescription>Launcher configuration.</DialogDescription>
        </DialogHeader>

        {error && <p className="text-xs text-destructive">{error}</p>}

        <div className="flex flex-col gap-5">
          <div>
            <label
              htmlFor="launch-args"
              className="mb-1.5 block text-[10px] font-medium uppercase text-muted-foreground"
            >
              Launch Arguments
            </label>
            <input
              id="launch-args"
              value={launchArgs}
              onChange={(event) => setLaunchArgs(event.target.value)}
              spellCheck={false}
              className={inputClass}
            />
          </div>
          <div>
            <Tooltip>
              <TooltipTrigger>
                <label
                  htmlFor="release-branch"
                  className="mb-1.5 block text-[10px] font-medium uppercase text-muted-foreground"
                >
                  Release Branch
                </label>
              </TooltipTrigger>
              <TooltipContent side="right">
                <p>For advanced use cases only - changes the branch of LJE you download.</p>
              </TooltipContent>
            </Tooltip>
            <input
              id="release-branch"
              value={releaseBranch}
              onChange={(event) => setReleaseBranch(event.target.value)}
              spellCheck={false}
              className={inputClass}
            />
          </div>
        </div>

        <div className="flex gap-2 pt-2">
          <Button onClick={() => void save()} className="w-full flex-[7]">
            Save
          </Button>
          <Button
            variant="secondary"
            onClick={() => onOpenChange(false)}
            className="w-full flex-[3]"
          >
            Discard
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
