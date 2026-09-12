use domain::entities::{CreateHelpArticle, CreateHelpCategory, HelpArticle, HelpCategory};
use domain::enums::HelpArticleStatus;
use domain::shared::ids::{HelpArticleId, HelpCategoryId};

#[test]
fn help_category_accepts_valid_input() {
    let category = HelpCategory::create(CreateHelpCategory {
        id: HelpCategoryId::new(),
        name: "Getting Started".into(),
        slug: "getting-started".into(),
        description: "Intro".into(),
        sort_order: 1,
    })
    .unwrap();
    assert_eq!(category.name, "Getting Started");
}

#[test]
fn help_category_rejects_empty_name() {
    let result = HelpCategory::create(CreateHelpCategory {
        id: HelpCategoryId::new(),
        name: "".into(),
        slug: "x".into(),
        description: "".into(),
        sort_order: 0,
    });
    assert!(matches!(
        result,
        Err(domain::errors::DomainError::EmptyHelpCategoryName)
    ));
}

#[test]
fn help_article_accepts_valid_input() {
    let article = HelpArticle::create(CreateHelpArticle {
        id: HelpArticleId::new(),
        category_id: HelpCategoryId::new(),
        title: "Monitor".into(),
        slug: "monitor".into(),
        content: "How to monitor".into(),
        status: HelpArticleStatus::Published,
    })
    .unwrap();
    assert_eq!(article.status, HelpArticleStatus::Published);
}

#[test]
fn help_article_rejects_empty_title() {
    let result = HelpArticle::create(CreateHelpArticle {
        id: HelpArticleId::new(),
        category_id: HelpCategoryId::new(),
        title: "".into(),
        slug: "x".into(),
        content: "content".into(),
        status: HelpArticleStatus::Draft,
    });
    assert!(matches!(
        result,
        Err(domain::errors::DomainError::EmptyHelpArticleTitle)
    ));
}
