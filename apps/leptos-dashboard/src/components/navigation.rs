//! Navigation model for the authenticated app shell.
//!
//! This module is intentionally data-only: it describes which product areas
//! exist, which route each one points at, and whether that route is
//! registered yet. `components::layout::Layout` renders this data. Keeping
//! the data separate from the render code means Path 10 (or any future
//! integration pass) can add a future navigation item in one place, in
//! `routes/mod.rs`-adjacent order.
//! without touching rendering logic.
//!
//! IMPORTANT: adding an item here does NOT register a route. A route must
//! exist in `routes/mod.rs` (owned by Path 10) before its `NavStatus` is
//! flipped to `Available`, otherwise the link would 404 or fall through to
//! `NotFoundPage`.

/// Whether a nav target is safe to click today.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NavStatus {
    /// The route is registered in `routes/mod.rs` and the page exists.
    Available,
}

#[derive(Clone, Copy)]
pub struct NavItem {
    pub label: &'static str,
    pub href: &'static str,
    pub status: NavStatus,
}

#[derive(Clone, Copy)]
pub struct NavGroup {
    pub label: &'static str,
    pub items: &'static [NavItem],
}

// Single source of truth for the sidebar. Update the `status` field (only)
// once the corresponding route lands in `routes/mod.rs` — do not add new
// hrefs speculatively; every href here must correspond to a page a Path has
// actually delivered or is actively delivering.
pub const NAV_GROUPS: &[NavGroup] = &[
    NavGroup {
        label: "OVERVIEW",
        items: &[NavItem {
            label: "Dashboard",
            href: "/",
            status: NavStatus::Available,
        }],
    },
    NavGroup {
        label: "TOOLS",
        items: &[NavItem {
            label: "Proxy Checker",
            href: "/proxy-check",
            status: NavStatus::Available,
        }],
    },
    NavGroup {
        label: "MONITORING",
        items: &[
            NavItem {
                label: "Monitors",
                href: "/monitors",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Bulk import",
                href: "/monitors/import",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Incidents",
                href: "/incidents",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Maintenance windows",
                href: "/maintenance",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Escalation policies",
                href: "/escalation-policies",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "USER EXPERIENCE",
        items: &[
            NavItem {
                label: "Real User Monitoring",
                href: "/rum",
                status: NavStatus::Available,
            },
            NavItem {
                label: "RUM sites",
                href: "/rum/sites",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "PUBLIC",
        items: &[NavItem {
            label: "Status pages",
            href: "/status-pages",
            status: NavStatus::Available,
        }],
    },
    NavGroup {
        label: "TEAM",
        items: &[
            NavItem {
                label: "Members",
                href: "/team",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Invitations",
                href: "/team/invitations",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "DEVELOPER",
        items: &[
            NavItem {
                label: "API keys",
                href: "/developer/api-keys",
                status: NavStatus::Available,
            },
            NavItem {
                label: "API flows",
                href: "/developer/api-flows",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "BILLING",
        items: &[
            NavItem {
                label: "PAYG billing",
                href: "/billing",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Add-ons",
                href: "/billing/addons",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Wallet",
                href: "/billing/wallet",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Usage",
                href: "/billing/usage",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Invoices",
                href: "/billing/invoices",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Transactions",
                href: "/billing/transactions",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Audit log",
                href: "/audit",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "SUPPORT",
        items: &[
            NavItem {
                label: "Help center",
                href: "/help",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Support tickets",
                href: "/support",
                status: NavStatus::Available,
            },
        ],
    },
    NavGroup {
        label: "PLATFORM",
        items: &[NavItem {
            label: "Administration",
            href: "/admin",
            status: NavStatus::Available,
        }],
    },
    NavGroup {
        label: "SETTINGS",
        items: &[
            NavItem {
                label: "Account",
                href: "/settings/account",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Security",
                href: "/settings/security",
                status: NavStatus::Available,
            },
            NavItem {
                label: "Notifications",
                href: "/settings/notifications",
                status: NavStatus::Available,
            },
        ],
    },
];
