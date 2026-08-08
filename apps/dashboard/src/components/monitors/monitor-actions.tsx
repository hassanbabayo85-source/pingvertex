"use client";

import type { MonitorDto } from "@/lib/api";

import { DeleteMonitorDialog } from "./delete-monitor-dialog";
import { EditMonitorDialog } from "./edit-monitor-dialog";

type MonitorActionsProps = {
  monitor: MonitorDto;
};

export function MonitorActions({ monitor }: MonitorActionsProps) {
  return (
    <div className="flex flex-wrap gap-3">
      <EditMonitorDialog monitor={monitor} />
      <DeleteMonitorDialog monitor={monitor} />
    </div>
  );
}
