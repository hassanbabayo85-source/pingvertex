use domain::entities::{CreateSupportMessage, CreateSupportTicket, SupportMessage, SupportTicket};
use domain::enums::{SupportCategory, SupportTicketStatus};
use domain::shared::ids::{SupportMessageId, SupportTicketId, TenantId, UserId};

#[test]
fn support_ticket_accepts_valid_input() {
    let ticket = SupportTicket::create(CreateSupportTicket {
        id: SupportTicketId::new(),
        ticket_number: "PV-ABCD1234".into(),
        tenant_id: TenantId::new(),
        created_by: UserId::new(),
        subject: "Help".into(),
        category: SupportCategory::Account,
        status: SupportTicketStatus::Open,
    })
    .unwrap();
    assert_eq!(ticket.status, SupportTicketStatus::Open);
}

#[test]
fn support_ticket_rejects_empty_subject() {
    let result = SupportTicket::create(CreateSupportTicket {
        id: SupportTicketId::new(),
        ticket_number: "PV-ABCD1234".into(),
        tenant_id: TenantId::new(),
        created_by: UserId::new(),
        subject: "".into(),
        category: SupportCategory::Other,
        status: SupportTicketStatus::Open,
    });
    assert!(matches!(
        result,
        Err(domain::errors::DomainError::EmptySupportSubject)
    ));
}

#[test]
fn support_ticket_status_transition_is_validated() {
    let mut ticket = SupportTicket::create(CreateSupportTicket {
        id: SupportTicketId::new(),
        ticket_number: "PV-ABCD1234".into(),
        tenant_id: TenantId::new(),
        created_by: UserId::new(),
        subject: "Help".into(),
        category: SupportCategory::Account,
        status: SupportTicketStatus::Open,
    })
    .unwrap();

    assert!(
        ticket
            .update_status(SupportTicketStatus::InProgress)
            .is_ok()
    );
    assert!(ticket.update_status(SupportTicketStatus::Open).is_err());
}

#[test]
fn support_message_accepts_valid_input() {
    let msg = SupportMessage::create(CreateSupportMessage {
        id: SupportMessageId::new(),
        ticket_id: SupportTicketId::new(),
        author_id: UserId::new(),
        message: "Hello".into(),
    })
    .unwrap();
    assert_eq!(msg.message, "Hello");
}
