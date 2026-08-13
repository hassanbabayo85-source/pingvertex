import axios from "axios";

const baseURL =
  process.env.NEXT_PUBLIC_API_URL ??
  process.env.API_URL ??
  "http://127.0.0.1:3000";

const api = axios.create({
  baseURL,
  timeout: 15_000,
  // Access token yanzu httpOnly cookie ne, don haka muna bukatar wannan
  // domin browser ya aika ta zuwa authenticated endpoints.
  withCredentials: true,
});

export type MonitorDto = {
  id: string;
  user_id: string;
  name: string;
  url: string;
  method: "GET" | "HEAD";
  expected_status: number;
  interval_seconds: number;
  timeout_seconds: number;
  enabled: boolean;
  is_public: boolean;
  created_at: string;
  updated_at: string;
};

export type CheckDto = {
  id: string;
  monitor_id: string;
  status: "Up" | "Down";
  status_code: number | null;
  latency_ms: number;
  error_type: string;
  error_message: string | null;
  checked_at: string;
};

export type IncidentDto = {
  id: string;
  monitor_id: string;
  started_at: string;
  resolved_at: string | null;
  created_at: string;
  updated_at: string;
};

export type CreateMonitorPayload = {
  user_id: string;
  name: string;
  url: string;
  method: "GET" | "HEAD";
  expected_status: number;
  interval_seconds: number;
  timeout_seconds: number;
  enabled: boolean;
  is_public: boolean;
};

export type PublicIncidentDto = {
  id: string;
  started_at: string;
  resolved_at: string | null;
};

// Bayanin da endpoint na public (mara-auth) ke dawowa — a hankali, KAWAI
// fields marasa hadari. Babu user_id ko assertions a nan, domin kowa zai
// iya ganin wannan bayanin ba tare da login ba.
export type PublicMonitorDto = {
  id: string;
  name: string;
  url: string;
  status: "Up" | "Down" | null;
  latency_ms: number | null;
  uptime_percentage: number;
  incidents: PublicIncidentDto[];
};

export async function getPublicStatus() {
  const { data } = await api.get<PublicMonitorDto[]>("/public/status");
  return data;
}

export async function getMonitors() {
  const { data } = await api.get<MonitorDto[]>("/monitors");
  return data;
}

export async function getMonitor(id: string) {
  const { data } = await api.get<MonitorDto>(`/monitors/${id}`);
  return data;
}

export async function getChecks(monitorId: string) {
  const { data } = await api.get<CheckDto[]>(`/monitors/${monitorId}/checks`);
  return data;
}

export async function getIncidents(monitorId: string) {
  const { data } = await api.get<IncidentDto[]>(
    `/monitors/${monitorId}/incidents`,
  );
  return data;
}

export async function createMonitor(payload: CreateMonitorPayload) {
  const { data } = await api.post<MonitorDto>("/monitors", payload);
  return data;
}

export async function updateMonitor(
  id: string,
  payload: CreateMonitorPayload,
) {
  const { data } = await api.put<MonitorDto>(`/monitors/${id}`, payload);
  return data;
}

export async function deleteMonitor(id: string) {
  await api.delete(`/monitors/${id}`);
}
