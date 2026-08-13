import Link from "next/link";
import {
  CheckCircle2,
  Clock3,
  Globe,
  LayoutDashboard,
  ShieldAlert,
} from "lucide-react";

import { IncidentHistory, type StatusIncident } from "@/components/status/incident-history";
import { OverallStatus } from "@/components/status/overall-status";
import { ServiceList, type StatusService } from "@/components/status/service-list";
import { Card, CardContent } from "@/components/ui/card";
import { getPublicStatus, type PublicMonitorDto } from "@/lib/api";

export const dynamic = "force-dynamic";

// Wannan shafi mara-auth ne (public), don haka yana amfani da
// getPublicStatus() kawai — endpoint wanda ke dawo da monitors da aka yiwa
// alama a matsayin `is_public` KAWAI, ba dukkan monitors na kowa ba.
async function loadStatusData(): Promise<PublicMonitorDto[]> {
  return getPublicStatus();
}

function formatDate(value: string | null) {
  if (!value) return "—";

  return new Intl.DateTimeFormat("en", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

export default async function StatusPage() {
  const data = await loadStatusData();

  const services: StatusService[] = data.map((item) => ({
    id: item.id,
    name: item.name,
    url: item.url,
    status: item.status ?? "Idle",
    latency: item.latency_ms,
    uptime: item.uptime_percentage,
  }));

  const incidents: StatusIncident[] = data
    .flatMap((item) =>
      item.incidents.map((incident) => ({
        id: incident.id,
        serviceName: item.name,
        started_at: incident.started_at,
        resolved_at: incident.resolved_at,
      })),
    )
    .sort(
      (a, b) =>
        new Date(b.started_at).getTime() - new Date(a.started_at).getTime(),
    )
    .slice(0, 10);

  const openIncidents = incidents.filter((incident) => incident.resolved_at === null).length;
  const downServices = services.filter((service) => service.status === "Down").length;
  const avgUptime =
    services.length === 0
      ? 100
      : services.reduce((sum, service) => sum + service.uptime, 0) / services.length;

  const variant =
    openIncidents === 0 && downServices === 0
      ? "operational"
      : downServices < services.length
        ? "degraded"
        : "outage";

  return (
    <main className="min-h-screen bg-muted/30">
      <div className="mx-auto max-w-7xl space-y-8 px-6 py-10">
        <div className="flex items-center justify-between gap-4">
          <div>
            <p className="text-sm text-muted-foreground">Public status page</p>
            <h1 className="text-3xl font-bold tracking-tight">
              Uptime Monitor Status
            </h1>
          </div>

          <Link
            href="/"
            className="inline-flex h-10 items-center justify-center rounded-md border bg-background px-4 py-2 text-sm font-medium shadow-sm hover:bg-muted"
          >
            <LayoutDashboard className="mr-2 h-4 w-4" />
            Dashboard
          </Link>
        </div>

        <OverallStatus
          title={
            openIncidents === 0 && downServices === 0
              ? "All Systems Operational"
              : "Service Degradation Detected"
          }
          subtitle="Live status from the Rust worker and monitor checks."
          variant={variant}
          uptime={avgUptime}
          openIncidents={openIncidents}
        />

        <div className="grid gap-4 md:grid-cols-3">
          <Card>
            <CardContent className="flex items-center gap-3 p-5">
              <Globe className="h-5 w-5 text-muted-foreground" />
              <div>
                <p className="text-sm text-muted-foreground">Services</p>
                <p className="text-2xl font-bold">{services.length}</p>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardContent className="flex items-center gap-3 p-5">
              <CheckCircle2 className="h-5 w-5 text-muted-foreground" />
              <div>
                <p className="text-sm text-muted-foreground">Operational</p>
                <p className="text-2xl font-bold">
                  {services.filter((service) => service.status === "Up").length}
                </p>
              </div>
            </CardContent>
          </Card>

          <Card>
            <CardContent className="flex items-center gap-3 p-5">
              <ShieldAlert className="h-5 w-5 text-muted-foreground" />
              <div>
                <p className="text-sm text-muted-foreground">Open incidents</p>
                <p className="text-2xl font-bold">{openIncidents}</p>
              </div>
            </CardContent>
          </Card>
        </div>

        <ServiceList services={services} />

        <IncidentHistory incidents={incidents} />

        <Card>
          <CardContent className="flex items-center gap-2 p-5 text-sm text-muted-foreground">
            <Clock3 className="h-4 w-4" />
            Last refresh: {formatDate(new Date().toISOString())}
          </CardContent>
        </Card>
      </div>
    </main>
  );
}
