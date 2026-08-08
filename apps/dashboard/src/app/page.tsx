

      <div className="mb-6 flex items-center justify-between">
        <div>
          <h1 className="text-3xl font-bold">
            Uptime Dashboard
          </h1>

          <p className="text-muted-foreground">
            Monitor your infrastructure.
          </p>
        </div>

        <UserMenu />
      </div>

import { DashboardContent } from "@/components/dashboard/dashboard-content";
import { UserMenu } from "@/components/auth/user-menu";
import {
  getChecks,
  getIncidents,
  getMonitors,
  type CheckDto,
  type IncidentDto,
  type MonitorDto,
} from "@/lib/api";

export const dynamic = "force-dynamic";

type MonitorOverview = {
  monitor: MonitorDto;
  latestCheck: CheckDto | null;
  incidents: IncidentDto[];
};

async function loadDashboard(): Promise<MonitorOverview[]> {
  const monitors = await getMonitors();

  return Promise.all(
    monitors.map(async (monitor) => {
      const [checks, incidents] = await Promise.all([
        getChecks(monitor.id),
        getIncidents(monitor.id),
      ]);

      return {
        monitor,
        latestCheck: checks[0] ?? null,
        incidents,
      };
    }),
  );
}

export default async function Page() {
  const monitors = await loadDashboard();

  return <DashboardContent monitors={monitors} />;
}
