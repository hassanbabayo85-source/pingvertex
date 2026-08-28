use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct HelpCategoryResponse {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub sort_order: i32,
}

#[derive(Debug, Serialize)]
pub struct HelpArticleResponse {
    pub id: String,
    pub category_id: String,
    pub title: String,
    pub slug: String,
    pub content: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<domain::entities::HelpCategory> for HelpCategoryResponse {
    fn from(c: domain::entities::HelpCategory) -> Self {
        Self {
            id: c.id.0.to_string(),
            name: c.name,
            slug: c.slug,
            description: c.description,
            sort_order: c.sort_order,
        }
    }
}

impl From<domain::entities::HelpArticle> for HelpArticleResponse {
    fn from(a: domain::entities::HelpArticle) -> Self {
        Self {
            id: a.id.0.to_string(),
            category_id: a.category_id.0.to_string(),
            title: a.title,
            slug: a.slug,
            content: a.content,
            status: a.status.as_str().to_string(),
            created_at: a.created_at.to_rfc3339(),
            updated_at: a.updated_at.to_rfc3339(),
        }
    }
}
