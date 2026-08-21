import { useState } from "react";
import * as api from "@/lib/api";
import { useStore } from "@/lib/store";
import { toast } from "@/components/ui/toast";

export function useRecording() {
  const recording = useStore((s) => s.recording);
  const [busy, setBusy] = useState(false);

  const toggle = async () => {
    if (busy) return;
    setBusy(true);
    try {
      if (recording) {
        try {
          const result = await api.stopRecording();
          if (result?.text) useStore.setState({ lastResult: result });
        } catch (e) {
          toast.error(`Stop failed: ${String(e)}`);
        } finally {
          useStore.getState().setRecording(false);
        }
      } else {
        try {
          await api.startRecording("dictate");
          useStore.getState().setRecording(true);
        } catch (e) {
          toast.error(`Could not start recording: ${String(e)}`);
        }
      }
    } finally {
      setBusy(false);
    }
  };

  return { recording, busy, toggle };
}
