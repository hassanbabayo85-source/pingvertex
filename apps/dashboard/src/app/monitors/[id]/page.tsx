import { notFound } from "next/navigation";

import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { MonitorActions } from "@/components/monitors/monitor-actions";
import { LatencyChart } from "@/components/monitors/latency-chart";
import {
  getChecks,
  getIncidents,
  getMonitor,
  type CheckDto,
  type IncidentDto,
} from "@/lib/api";
import { cn } from "@/lib/utils";

export const dynamic = "force-dynamic";

type PageProps = {
  params: Promise<{
    id: string;
  }>;
};

type MonitorData = Awaited<ReturnType<typeof getMonitor>>;

function formatDate(value: string | null | undefined) {
  if (!value) return "—";
  return new Intl.DateTimeFormat("en", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

function formatLatency(latencyMs: number | null) {
  if (latencyMs === null) return "—";
  return `${latencyMs} ms`;
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

async function loadMonitorData(id: string) {
  const [monitor, checks, incidents] = await Promise.all([
    getMonitor(id),
    getChecks(id),
    getIncidents(id),
  ]);

  return { monitor, checks, incidents };
}

function getLatestCheck(checks: CheckDto[]) {
  return checks[0] ?? null;
}

function getOpenIncident(incidents: IncidentDto[]) {
  return incidents.find((incident) => incident.resolved_at === null) ?? null;
}

export default async function MonitorDetailsPage({ params }: PageProps) {
  const { id } = await params;
  const { monitor, checks, incidents } = await loadMonitorData(id);

  if (!monitor) {
    notFound();
  }

  const currentMonitor = monitor as NonNullable<MonitorData>;
  const latestCheck = getLatestCheck(checks);
  const openIncident = getOpenIncident(incidents);
  const status = latestCheck?.status ?? "Idle";

  const chartData = [...checks]
    .slice(0, 20)
    .reverse()
    .map((check) => ({
      time: check.checked_at,
      latency: check.latency_ms,
      status: check.status,
    }));

  return (
    <main className="mx-auto max-w-6xl space-y-8 p-8">
      <section className="space-y-4">
        <div className="flex flex-col gap-4 lg:flex-row lg:items-start lg:justify-between">
          <div>
            <p className="text-sm text-muted-foreground">Monitor details</p>
            <h1 className="text-3xl font-bold tracking-tight">
              {currentMonitor.name}
            </h1>
            <p className="mt-2 text-muted-foreground">{currentMonitor.url}</p>
          </div>

          <MonitorActions monitor={currentMonitor} />
        </div>

        <div className="grid gap-4 md:grid-cols-2 xl:grid-cols-4">
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium">Status</CardTitle>
            </CardHeader>
            <CardContent>
              <StatusBadge status={status} />
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium">
                Expected status
              </CardTitle>
            </CardHeader>
            <CardContent className="text-2xl font-bold">
              {currentMonitor.expected_status}
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium">Interval</CardTitle>
            </CardHeader>
            <CardContent className="text-2xl font-bold">
              {currentMonitor.interval_seconds}s
            </CardContent>
          </Card>

          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-medium">Timeout</CardTitle>
            </CardHeader>
            <CardContent className="text-2xl font-bold">
              {currentMonitor.timeout_seconds}s
            </CardContent>
          </Card>
        </div>
      </section>

      <section className="grid gap-6 xl:grid-cols-2">
        <Card>
          <CardHeader>
            <CardTitle>Monitor summary</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-sm">
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Method</span>
              <span className="font-medium">{currentMonitor.method}</span>
            </div>
            <Separator />
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Enabled</span>
              <span className="font-medium">
                {currentMonitor.enabled ? "Yes" : "No"}
              </span>
            </div>
            <Separator />
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Created</span>
              <span className="font-medium">
                {formatDate(currentMonitor.created_at)}
              </span>
            </div>
            <Separator />
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Updated</span>
              <span className="font-medium">
                {formatDate(currentMonitor.updated_at)}
              </span>
            </div>
            <Separator />
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Latest latency</span>
              <span className="font-medium">
                {latestCheck ? formatLatency(latestCheck.latency_ms) : "—"}
              </span>
            </div>
            <Separator />
            <div className="flex items-center justify-between">
              <span className="text-muted-foreground">Open incident</span>
              <span className="font-medium">{openIncident ? "Yes" : "None"}</span>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardHeader>
            <CardTitle>Current check details</CardTitle>
          </CardHeader>
          <CardContent className="space-y-3 text-sm">
            {latestCheck ? (
              <>
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">Status</span>
                  <StatusBadge status={latestCheck.status} />
                </div>
                <Separator />
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">Status code</span>
                  <span className="font-medium">
                    {latestCheck.status_code ?? "—"}
                  </span>
                </div>
                <Separator />
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">Latency</span>
                  <span className="font-medium">
                    {formatLatency(latestCheck.latency_ms)}
                  </span>
                </div>
                <Separator />
                <div className="flex items-center justify-between">
                  <span className="text-muted-foreground">Error type</span>
                  <span className="font-medium">{latestCheck.error_type}</span>
                </div>
                <Separator />
                <div className="flex flex-col gap-1">
                  <span className="text-muted-foreground">Error message</span>
                  <span className="font-medium">
                    {latestCheck.error_message ?? "—"}
                  </span>
                </div>
              </>
            ) : (
              <p className="text-muted-foreground">No checks yet.</p>
            )}
          </CardContent>
        </Card>
      </section>

      <section className="grid gap-6 xl:grid-cols-2">
        <LatencyChart data={chartData} />

        <Card>
          <CardHeader>
            <CardTitle>Incidents</CardTitle>
          </CardHeader>
          <CardContent className="overflow-x-auto p-0">
            {incidents.length === 0 ? (
              <div className="p-6 text-sm text-muted-foreground">
                No incidents recorded yet.
              </div>
            ) : (
              <table className="w-full min-w-[700px]">
                <thead className="border-b bg-muted/40 text-left text-sm">
                  <tr>
                    <th className="px-6 py-4 font-medium">Started</th>
                    <th className="px-6 py-4 font-medium">Resolved</th>
                    <th className="px-6 py-4 font-medium">Status</th>
                  </tr>
                </thead>
                <tbody>
                  {incidents.map((incident) => (
                    <tr key={incident.id} className="border-b last:border-b-0">
                      <td className="px-6 py-4 text-sm">
                        {formatDate(incident.started_at)}
                      </td>
                      <td className="px-6 py-4 text-sm">
                        {formatDate(incident.resolved_at)}
                      </td>
                      <td className="px-6 py-4 text-sm">
                        <Badge
                          className={cn(
                            "rounded-full px-3 py-1 font-medium",
                            incident.resolved_at
                              ? "border-emerald-500/30 bg-emerald-500/15 text-emerald-700"
                              : "border-rose-500/30 bg-rose-500/15 text-rose-700",
                          )}
                        >
                          {incident.resolved_at ? "Resolved" : "Open"}
                        </Badge>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </CardContent>
        </Card>
      </section>
    </main>
  );
}
