"use client";

import { useMemo, useState } from "react";
import Link from "next/link";
import {
  AlertTriangle,
  ArrowUpRight,
  Clock3,
  Globe,
  LayoutDashboard,
  ShieldAlert,
} from "lucide-react";

import { DashboardSummary } from "@/components/dashboard/dashboard-summary";
import { IncidentTimeline } from "@/components/dashboard/incident-timeline";
import { MonitorToolbar } from "@/components/dashboard/monitor-toolbar";
import { UptimeCard } from "@/components/dashboard/uptime-card";
import { Header } from "@/components/layout/header";
import { Sidebar } from "@/components/layout/sidebar";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";
import { LatencyOverviewChart } from "@/components/dashboard/latency-overview-chart";

type MonitorOverview = {
  monitor: {
    id: string;
    name: string;
    url: string;
    method: string;
    interval_seconds: number;
  };
  latestCheck: {
    status?: "Up" | "Down" | "Idle";
    latency_ms?: number | null;
    checked_at?: string;
    status_code?: number | null;
  } | null;
  incidents: {
    id: string;
    started_at: string;
    resolved_at: string | null;
    created_at: string;
    updated_at: string;
  }[];
};

type Props = {
  monitors: MonitorOverview[];
};

function formatLatency(latencyMs: number | null) {
  if (latencyMs === null) return "—";
  return `${latencyMs} ms`;
}

function formatDate(value: string | null) {
  if (!value) return "—";

  return new Intl.DateTimeFormat("en", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

function statusClass(status?: "Up" | "Down" | "Idle") {
  switch (status) {
    case "Up":
      return "border-emerald-500/30 bg-emerald-500/15 text-emerald-700";
    case "Down":
      return "border-rose-500/30 bg-rose-500/15 text-rose-700";
    default:
      return "border-border bg-muted text-muted-foreground";
  }
}

function StatusBadge({ status }: { status?: "Up" | "Down" | "Idle" }) {
  return (
    <Badge className={cn("rounded-full px-3 py-1 font-medium", statusClass(status))}>
      {status ?? "Idle"}
    </Badge>
  );
}

export function DashboardContent({ monitors }: Props) {
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState("all");

  const filteredMonitors = useMemo(() => {
    return monitors.filter((item) => {
      const query = search.trim().toLowerCase();

      const matchesSearch =
        query.length === 0 ||
        item.monitor.name.toLowerCase().includes(query) ||
        item.monitor.url.toLowerCase().includes(query);

      const currentStatus = item.latestCheck?.status?.toLowerCase() ?? "idle";
      const matchesStatus = status === "all" || currentStatus === status;

      return matchesSearch && matchesStatus;
    });
  }, [monitors, search, status]);

  const summaryMonitors = filteredMonitors.map((item) => ({
    id: item.monitor.id,
    status: item.latestCheck?.status,
    latency_ms: item.latestCheck?.latency_ms ?? null,
  }));

  const latencyChartData = filteredMonitors
    .map((item) => ({
      name: item.monitor.name,
      latency: item.latestCheck?.latency_ms ?? 0,
    }))
    .filter((item) => item.latency > 0);

  const uptimePercentage =
    filteredMonitors.length === 0
      ? 100
      : (() => {
          let total = 0;
          let up = 0;

          for (const monitor of filteredMonitors) {
            total++;
            if (monitor.latestCheck?.status === "Up") {
              up++;
            }
          }

          return total === 0 ? 100 : (up / total) * 100;
        })();

  const totalMonitors = filteredMonitors.length;
  const healthyMonitors = filteredMonitors.filter(
    (item) => item.latestCheck?.status === "Up",
  ).length;
  const downMonitors = filteredMonitors.filter(
    (item) => item.latestCheck?.status === "Down",
  ).length;
  const openIncidents = filteredMonitors.flatMap((item) =>
    item.incidents.filter((incident) => incident.resolved_at === null),
  ).length;

  const recentIncidents = filteredMonitors
    .flatMap((item) => item.incidents)
    .sort(
      (a, b) =>
        new Date(b.started_at).getTime() -
        new Date(a.started_at).getTime(),
    )
    .slice(0, 10);

  return (
    <div className="flex min-h-screen bg-muted/30">
      <Sidebar />

      <div className="flex min-w-0 flex-1 flex-col">
        <Header />

        <main className="space-y-8 p-6">
          <section id="dashboard" className="space-y-6">
            <div>
              <p className="text-sm text-muted-foreground">Overview</p>
              <h2 className="text-3xl font-semibold tracking-tight">
                System status at a glance
              </h2>
            </div>

            <DashboardSummary monitors={summaryMonitors} />

            <div className="grid gap-4 lg:grid-cols-2">
              <UptimeCard uptime={uptimePercentage} />
              <IncidentTimeline incidents={recentIncidents} />
            </div>

            <LatencyOverviewChart data={latencyChartData} />

            <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
              <Card>
                <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                  <CardTitle className="text-sm font-medium">
                    Total Monitors
                  </CardTitle>
                  <Globe className="h-4 w-4 text-muted-foreground" />
                </CardHeader>
                <CardContent>
                  <div className="text-3xl font-bold">{totalMonitors}</div>
                  <p className="text-xs text-muted-foreground">
                    All monitors currently tracked.
                  </p>
                </CardContent>
              </Card>

              <Card>
                <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                  <CardTitle className="text-sm font-medium">Healthy</CardTitle>
                  <LayoutDashboard className="h-4 w-4 text-muted-foreground" />
                </CardHeader>
                <CardContent>
                  <div className="text-3xl font-bold text-emerald-600">
                    {healthyMonitors}
                  </div>
                  <p className="text-xs text-muted-foreground">
                    Latest check returned Up.
                  </p>
                </CardContent>
              </Card>

              <Card>
                <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                  <CardTitle className="text-sm font-medium">Down</CardTitle>
                  <AlertTriangle className="h-4 w-4 text-muted-foreground" />
                </CardHeader>
                <CardContent>
                  <div className="text-3xl font-bold text-rose-600">
                    {downMonitors}
                  </div>
                  <p className="text-xs text-muted-foreground">
                    Latest check returned Down.
                  </p>
                </CardContent>
              </Card>

              <Card>
                <CardHeader className="flex flex-row items-center justify-between space-y-0 pb-2">
                  <CardTitle className="text-sm font-medium">
                    Open Incidents
                  </CardTitle>
                  <ShieldAlert className="h-4 w-4 text-muted-foreground" />
                </CardHeader>
                <CardContent>
                  <div className="text-3xl font-bold">{openIncidents}</div>
                  <p className="text-xs text-muted-foreground">
                    Incidents waiting for recovery.
                  </p>
                </CardContent>
              </Card>
            </div>
          </section>

          <section id="monitors" className="space-y-4">
            <div className="flex flex-col gap-4 md:flex-row md:items-end md:justify-between">
              <div>
                <p className="text-sm text-muted-foreground">Monitors</p>
                <h3 className="text-2xl font-semibold">Current monitors</h3>
              </div>

              <MonitorToolbar
                search={search}
                status={status}
                onSearchChange={setSearch}
                onStatusChange={(value) => setStatus(value ?? "all")}
              />
            </div>

            <Card className="overflow-hidden">
              <CardContent className="p-0">
                {filteredMonitors.length === 0 ? (
                  <div className="p-8 text-center text-sm text-muted-foreground">
                    No monitors match your search or filter.
                  </div>
                ) : (
                  <div className="overflow-x-auto">
                    <table className="w-full min-w-[900px]">
                      <thead className="border-b bg-muted/40 text-left text-sm">
                        <tr>
                          <th className="px-6 py-4 font-medium">Status</th>
                          <th className="px-6 py-4 font-medium">Name</th>
                          <th className="px-6 py-4 font-medium">URL</th>
                          <th className="px-6 py-4 font-medium">Last Check</th>
                          <th className="px-6 py-4 font-medium">Latency</th>
                          <th className="px-6 py-4 font-medium">Incidents</th>
                          <th className="px-6 py-4 font-medium text-right">
                            Actions
                          </th>
                        </tr>
                      </thead>
                      <tbody>
                        {filteredMonitors.map(({ monitor, latestCheck, incidents }) => {
                          const currentStatus = latestCheck?.status ?? "Idle";
                          const openIncident = incidents.find(
                            (incident) => incident.resolved_at === null,
                          );

                          return (
                            <tr key={monitor.id} className="border-b last:border-b-0">
                              <td className="px-6 py-4">
                                <StatusBadge status={currentStatus} />
                              </td>
                              <td className="px-6 py-4">
                                <Link
                                  href={`/monitors/${monitor.id}`}
                                  className="font-medium hover:underline"
                                >
                                  {monitor.name}
                                </Link>
                                <div className="text-xs text-muted-foreground">
                                  {monitor.method} · every {monitor.interval_seconds}s
                                </div>
                              </td>
                              <td className="px-6 py-4 text-sm text-muted-foreground">
                                {monitor.url}
                              </td>
                              <td className="px-6 py-4 text-sm">
                                {latestCheck
                                  ? formatDate(latestCheck.checked_at ?? null)
                                  : "—"}
                              </td>
                              <td className="px-6 py-4 text-sm">
                                {latestCheck
                                  ? formatLatency(latestCheck.latency_ms ?? null)
                                  : "—"}
                              </td>
                              <td className="px-6 py-4">
                                {openIncident ? (
                                  <Badge className="rounded-full border-rose-500/30 bg-rose-500/15 px-3 py-1 text-rose-700">
                                    Open
                                  </Badge>
                                ) : (
                                  <Badge variant="secondary">None</Badge>
                                )}
                              </td>
                              <td className="px-6 py-4 text-right text-sm">
                                <Link
                                  href={`/monitors/${monitor.id}`}
                                  className="inline-flex items-center gap-1 text-primary hover:underline"
                                >
                                  View <ArrowUpRight className="h-3.5 w-3.5" />
                                </Link>
                              </td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                )}
              </CardContent>
            </Card>
          </section>

          <section id="incidents" className="space-y-4">
            <div>
              <p className="text-sm text-muted-foreground">Incidents</p>
              <h3 className="text-2xl font-semibold">Recent incidents</h3>
            </div>

            <Card>
              <CardContent className="space-y-4 p-6">
                {recentIncidents.length === 0 ? (
                  <p className="text-sm text-muted-foreground">
                    No incidents yet. This is a good sign.
                  </p>
                ) : (
                  recentIncidents.map((incident) => (
                    <div
                      key={incident.id}
                      className="flex flex-col gap-2 rounded-2xl border p-4 md:flex-row md:items-center md:justify-between"
                    >
                      <div>
                        <div className="font-medium">
                          Incident #{incident.id.slice(0, 8)}
                        </div>
                        <div className="text-sm text-muted-foreground">
                          {incident.resolved_at ? "Resolved" : "Open"}
                        </div>
                      </div>
                      <div className="grid gap-2 text-sm md:text-right">
                        <div>
                          Started:{" "}
                          <span className="font-medium">
                            {formatDate(incident.started_at)}
                          </span>
                        </div>
                        <div>
                          Resolved:{" "}
                          <span className="font-medium">
                            {formatDate(incident.resolved_at)}
                          </span>
                        </div>
                      </div>
                    </div>
                  ))
                )}
              </CardContent>
            </Card>
          </section>

          <section id="activity" className="space-y-4">
            <div>
              <p className="text-sm text-muted-foreground">Activity</p>
              <h3 className="text-2xl font-semibold">Latest check activity</h3>
            </div>

            <Card>
              <CardHeader>
                <CardTitle className="text-base">Live polling status</CardTitle>
              </CardHeader>
              <CardContent className="space-y-3">
                <div className="flex items-center gap-2 text-sm text-muted-foreground">
                  <Clock3 className="h-4 w-4" />
                  Checks are being pulled from the Rust worker every 30 seconds.
                </div>
                <Separator />
                <div className="text-sm text-muted-foreground">
                  Use the API to add more monitors and the table above will update
                  automatically after refresh.
                </div>
              </CardContent>
            </Card>
          </section>

          <section id="settings" className="space-y-4 pb-10">
            <div>
              <p className="text-sm text-muted-foreground">Settings</p>
              <h3 className="text-2xl font-semibold">Foundation complete</h3>
            </div>

            <Card>
              <CardContent className="p-6 text-sm text-muted-foreground">
                Backend, worker, PostgreSQL, and the first dashboard view are
                connected. The next step is create/edit forms and real-time
                interactions.
              </CardContent>
            </Card>
          </section>
        </main>
      </div>
    </div>
  );
}
