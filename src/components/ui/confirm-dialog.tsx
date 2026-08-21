import { create } from "zustand";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";

interface ConfirmOptions {
  title: string;
  description?: string;
  confirmLabel?: string;
  destructive?: boolean;
}

interface ConfirmState extends ConfirmOptions {
  open: boolean;
  resolve: ((ok: boolean) => void) | null;
}

export const useConfirmStore = create<ConfirmState>()(() => ({
  open: false,
  title: "",
  description: undefined,
  confirmLabel: "Confirm",
  destructive: false,
  resolve: null,
}));

export function confirmDialog(opts: ConfirmOptions): Promise<boolean> {
  return new Promise((resolve) => {
    useConfirmStore.setState({ ...opts, open: true, resolve });
  });
}

function settle(ok: boolean): void {
  const { resolve } = useConfirmStore.getState();
  useConfirmStore.setState({ open: false });
  resolve?.(ok);
}

export function ConfirmDialogHost() {
  const { open, title, description, confirmLabel, destructive } = useConfirmStore();
  return (
    <Dialog open={open} onOpenChange={(o) => !o && settle(false)}>
      <DialogContent className="max-w-sm">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          {description && <DialogDescription>{description}</DialogDescription>}
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => settle(false)}>
            Cancel
          </Button>
          <Button
            variant={destructive ? "destructive" : "default"}
            onClick={() => settle(true)}
          >
            {confirmLabel ?? (destructive ? "Delete" : "Confirm")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
