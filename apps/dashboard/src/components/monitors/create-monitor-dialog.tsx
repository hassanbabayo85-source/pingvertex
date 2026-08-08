"use client";

import { useState } from "react";
import { PlusCircle } from "lucide-react";
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
import { useCreateMonitor } from "@/hooks/use-create-monitor";

import { MonitorForm } from "./monitor-form";
import {
  buildCreateMonitorDefaults,
  createMonitorSchema,
  type CreateMonitorValues,
} from "./monitor-schema";

export function CreateMonitorDialog() {
  const [open, setOpen] = useState(false);
  const [userId] = useState(() => crypto.randomUUID());

  const defaults = buildCreateMonitorDefaults(userId);

  const form = useForm<CreateMonitorValues>({
    resolver: zodResolver(createMonitorSchema),
    defaultValues: defaults,
  });

  const mutation = useCreateMonitor(() => {
    setOpen(false);
    form.reset(defaults);
  });

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        if (!next) {
          form.reset(defaults);
        }
      }}
    >
      <Button type="button" onClick={() => setOpen(true)}>
        <PlusCircle className="mr-2 h-4 w-4" />
        New Monitor
      </Button>

      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Create Monitor</DialogTitle>
          <DialogDescription>
            Add a new endpoint to be checked by the worker every interval.
          </DialogDescription>
        </DialogHeader>

        <MonitorForm
          form={form}
          onSubmit={(values) => mutation.mutate(values)}
          onCancel={() => setOpen(false)}
          loading={mutation.isPending}
        />
      </DialogContent>
    </Dialog>
  );
}
