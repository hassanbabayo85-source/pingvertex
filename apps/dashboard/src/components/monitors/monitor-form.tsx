"use client";

import type { UseFormReturn } from "react-hook-form";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";

import type { CreateMonitorValues } from "./monitor-schema";

type MonitorFormProps = {
  form: UseFormReturn<CreateMonitorValues>;
  onSubmit: (values: CreateMonitorValues) => void;
  onCancel: () => void;
  loading: boolean;
  submitLabel?: string;
};

const fieldClassName =
  "mt-2 h-11 rounded-xl border bg-background px-4 shadow-none focus-visible:ring-2 focus-visible:ring-ring";

function FieldError({ message }: { message?: string }) {
  if (!message) return null;

  return <p className="mt-2 text-xs text-rose-500">{message}</p>;
}

export function MonitorForm({
  form,
  onSubmit,
  onCancel,
  loading,
  submitLabel = "Create Monitor",
}: MonitorFormProps) {
  const { register, formState } = form;
  const { errors } = formState;

  return (
    <form onSubmit={form.handleSubmit(onSubmit)} className="space-y-6">
      <input type="hidden" {...register("user_id")} />

      <div className="space-y-2">
        <Label htmlFor="name">Monitor name</Label>
        <Input
          id="name"
          placeholder="Google Production"
          className={cn(fieldClassName, errors.name && "border-rose-500")}
          {...register("name")}
          disabled={loading}
        />
        <FieldError message={errors.name?.message} />
      </div>

      <div className="space-y-2">
        <Label htmlFor="url">URL</Label>
        <Input
          id="url"
          placeholder="https://example.com"
          className={cn(fieldClassName, errors.url && "border-rose-500")}
          {...register("url")}
          disabled={loading}
        />
        <FieldError message={errors.url?.message} />
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <div className="space-y-2">
          <Label htmlFor="method">HTTP method</Label>
          <select
            id="method"
            className={cn(
              fieldClassName,
              "w-full",
              errors.method && "border-rose-500",
            )}
            {...register("method")}
            disabled={loading}
          >
            <option value="GET">GET</option>
            <option value="HEAD">HEAD</option>
          </select>
          <FieldError message={errors.method?.message} />
        </div>

        <div className="space-y-2">
          <Label htmlFor="expected_status">Expected status</Label>
          <Input
            id="expected_status"
            type="number"
            min={100}
            max={599}
            className={cn(
              fieldClassName,
              errors.expected_status && "border-rose-500",
            )}
            {...register("expected_status", { valueAsNumber: true })}
            disabled={loading}
          />
          <FieldError message={errors.expected_status?.message} />
        </div>

        <div className="space-y-2">
          <Label htmlFor="interval_seconds">Check interval (seconds)</Label>
          <Input
            id="interval_seconds"
            type="number"
            min={10}
            className={cn(
              fieldClassName,
              errors.interval_seconds && "border-rose-500",
            )}
            {...register("interval_seconds", { valueAsNumber: true })}
            disabled={loading}
          />
          <FieldError message={errors.interval_seconds?.message} />
        </div>

        <div className="space-y-2">
          <Label htmlFor="timeout_seconds">Timeout (seconds)</Label>
          <Input
            id="timeout_seconds"
            type="number"
            min={1}
            className={cn(
              fieldClassName,
              errors.timeout_seconds && "border-rose-500",
            )}
            {...register("timeout_seconds", { valueAsNumber: true })}
            disabled={loading}
          />
          <FieldError message={errors.timeout_seconds?.message} />
        </div>
      </div>

      <div className="flex items-center gap-3">
        <input
          id="enabled"
          type="checkbox"
          className="h-4 w-4 rounded border-border"
          {...register("enabled")}
          disabled={loading}
        />
        <Label htmlFor="enabled" className="text-sm font-normal">
          Enable monitor
        </Label>
      </div>

      <div className="flex items-center gap-3">
        <input
          id="is_public"
          type="checkbox"
          className="h-4 w-4 rounded border-border"
          {...register("is_public")}
          disabled={loading}
        />
        <Label htmlFor="is_public" className="text-sm font-normal">
          Show on public status page
        </Label>
      </div>

      <div className="flex items-center justify-end gap-3 pt-2">
        <Button type="button" variant="outline" onClick={onCancel} disabled={loading}>
          Cancel
        </Button>
        <Button type="submit" disabled={loading}>
          {loading ? "Creating..." : submitLabel}
        </Button>
      </div>
    </form>
  );
}
