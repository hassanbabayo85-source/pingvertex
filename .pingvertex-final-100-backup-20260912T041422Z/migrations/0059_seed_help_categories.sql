-- Seed the ten Help Center categories required by H14.
-- These are metadata (names/slugs/order), not invented documentation.

INSERT INTO help_categories (id, name, slug, description, sort_order, created_at, updated_at)
VALUES
    (gen_random_uuid(), 'Getting Started', 'getting-started', 'First steps for new PingVertex users', 10, now(), now()),
    (gen_random_uuid(), 'Monitors', 'monitors', 'Creating and managing monitors', 20, now(), now()),
    (gen_random_uuid(), 'Alerts & Notifications', 'alerts-notifications', 'Configuring alerts and notification channels', 30, now(), now()),
    (gen_random_uuid(), 'Incidents & Downtime', 'incidents-downtime', 'Handling incidents and downtime events', 40, now(), now()),
    (gen_random_uuid(), 'Uptime & Performance', 'uptime-performance', 'Understanding uptime metrics and performance data', 50, now(), now()),
    (gen_random_uuid(), 'API', 'api', 'Working with the PingVertex API', 60, now(), now()),
    (gen_random_uuid(), 'Account & Security', 'account-security', 'Account settings, 2FA, and security', 70, now(), now()),
    (gen_random_uuid(), 'Billing & Plans', 'billing-plans', 'Plans, billing, and subscriptions', 80, now(), now()),
    (gen_random_uuid(), 'Troubleshooting', 'troubleshooting', 'Common issues and how to fix them', 90, now(), now()),
    (gen_random_uuid(), 'FAQ', 'faq', 'Frequently asked questions', 100, now(), now())
ON CONFLICT (slug) DO NOTHING;
