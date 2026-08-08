"use client";

import { Search } from "lucide-react";

import { CreateMonitorDialog } from "@/components/monitors/create-monitor-dialog";
import { ThemeToggle } from "@/components/theme-toggle";
import { Input } from "@/components/ui/input";

export function Header() {
  return (
    <header className="sticky top-0 z-20 border-b bg-background/80 backdrop-blur">
      <div className="flex h-16 items-center justify-between gap-4 px-6">
        <div>
          <p className="text-xs uppercase tracking-[0.24em] text-muted-foreground">
            Workspace
          </p>
          <h1 className="text-lg font-semibold">Uptime Monitor</h1>
        </div>

        <div className="flex items-center gap-3">
          <div className="hidden items-center gap-2 rounded-full border bg-muted/40 px-3 py-2 md:flex">
            <Search className="h-4 w-4 text-muted-foreground" />
            <Input
              className="h-8 w-64 border-0 bg-transparent p-0 shadow-none focus-visible:ring-0"
              placeholder="Search monitors"
            />
          </div>

          <ThemeToggle />
          <CreateMonitorDialog />
        </div>
      </div>
    </header>
  );
}
