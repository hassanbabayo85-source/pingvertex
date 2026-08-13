"use client";

import { useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import { PencilLine } from "lucide-react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useUpdateMonitor } from "@/hooks/use-update-monitor";
import type { MonitorDto } from "@/lib/api";

import { MonitorForm } from "./monitor-form";
import {
  createMonitorSchema,
  type CreateMonitorValues,
} from "./monitor-schema";

type EditMonitorDialogProps = {
  monitor: MonitorDto;
};

export function EditMonitorDialog({ monitor }: EditMonitorDialogProps) {
  const [open, setOpen] = useState(false);
  const router = useRouter();

  const defaults = useMemo<CreateMonitorValues>(
    () => ({
      user_id: monitor.user_id,
      name: monitor.name,
      url: monitor.url,
      method: monitor.method,
      expected_status: monitor.expected_status,
      interval_seconds: monitor.interval_seconds,
      timeout_seconds: monitor.timeout_seconds,
      enabled: monitor.enabled,
      is_public: monitor.is_public,
    }),
    [monitor],
  );

  const form = useForm<CreateMonitorValues>({
    resolver: zodResolver(createMonitorSchema),
    defaultValues: defaults,
  });

  const mutation = useUpdateMonitor(() => {
    setOpen(false);
    form.reset(defaults);
    router.refresh();
  });

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) {
          form.reset(defaults);
        } else {
          form.reset(defaults);
        }
      }}
    >
      <Button type="button" variant="outline" onClick={() => setOpen(true)}>
        <PencilLine className="mr-2 h-4 w-4" />
        Edit Monitor
      </Button>

      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Edit Monitor</DialogTitle>
          <DialogDescription>
            Update the endpoint configuration used by the worker.
          </DialogDescription>
        </DialogHeader>

        <MonitorForm
          form={form}
          onSubmit={(values) =>
            mutation.mutate({ id: monitor.id, payload: values })
          }
          onCancel={() => setOpen(false)}
          loading={mutation.isPending}
          submitLabel="Save Changes"
        />
      </DialogContent>
    </Dialog>
  );
}
