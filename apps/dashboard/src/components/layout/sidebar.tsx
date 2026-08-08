"use client";

import Link from "next/link";
import {
  Activity,
  Bell,
  Globe,
  LayoutDashboard,
  Settings,
  ShieldAlert,
} from "lucide-react";

import { cn } from "@/lib/utils";

const items = [
  { href: "/#dashboard", label: "Dashboard", icon: LayoutDashboard },
  { href: "/#monitors", label: "Monitors", icon: Globe },
  { href: "/#incidents", label: "Incidents", icon: ShieldAlert },
  { href: "/#activity", label: "Activity", icon: Activity },
  { href: "/#settings", label: "Settings", icon: Settings },
];

export function Sidebar() {
  return (
    <aside className="hidden w-72 shrink-0 border-r bg-background/90 lg:flex lg:flex-col">
      <div className="flex h-16 items-center border-b px-6">
        <div>
          <p className="text-xs uppercase tracking-[0.3em] text-muted-foreground">
            Uptime Monitor
          </p>
          <p className="text-sm font-medium">Dashboard</p>
        </div>
      </div>

      <nav className="flex-1 space-y-2 p-4">
        {items.map((item) => {
          const Icon = item.icon;

          return (
            <Link
              key={item.href}
              href={item.href}
              className={cn(
                "flex items-center gap-3 rounded-xl px-4 py-3 text-sm text-muted-foreground transition",
                "hover:bg-muted hover:text-foreground",
              )}
            >
              <Icon className="h-4 w-4" />
              {item.label}
            </Link>
          );
        })}
      </nav>

      <div className="border-t p-4">
        <div className="rounded-2xl border bg-muted/30 p-4">
          <div className="flex items-center gap-2 text-sm font-medium">
            <Bell className="h-4 w-4" />
            Live polling
          </div>
          <p className="mt-2 text-xs text-muted-foreground">
            Dashboard data refreshes every 30 seconds.
          </p>
        </div>
      </div>
    </aside>
  );
}
