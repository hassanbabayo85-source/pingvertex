import { z } from "zod";

export const createMonitorSchema = z.object({
  user_id: z.string().uuid("User ID must be a valid UUID"),
  name: z.string().trim().min(1, "Name is required").max(100),
  url: z.string().trim().url("URL must be a valid URL"),
  method: z.enum(["GET", "HEAD"]),
  expected_status: z.number()
    .int("Expected status must be an integer")
    .min(100, "Expected status must be at least 100")
    .max(599, "Expected status must be at most 599"),
  interval_seconds: z.number()
    .int("Interval must be an integer")
    .min(10, "Interval must be at least 10 seconds")
    .max(86_400, "Interval is too large"),
  timeout_seconds: z.number()
    .int("Timeout must be an integer")
    .min(1, "Timeout must be at least 1 second")
    .max(300, "Timeout is too large"),
  enabled: z.boolean(),
  // Idan wannan gaskiya ne (true), monitor din zai bayyana a /status (public
  // status page) ba tare da bukatar login ba. Default OFF domin user ya
  // zabi ta hannu wanda monitor zai fitowa a fili.
  is_public: z.boolean(),
});

export type CreateMonitorValues = z.infer<typeof createMonitorSchema>;

export function buildCreateMonitorDefaults(userId: string): CreateMonitorValues {
  return {
    user_id: userId,
    name: "",
    url: "",
    method: "GET",
    expected_status: 200,
    interval_seconds: 60,
    timeout_seconds: 10,
    enabled: true,
    is_public: false,
  };
}
