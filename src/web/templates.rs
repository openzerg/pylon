use crate::db::{DashboardStats, Permission, Proxy, RequestLog};
use askama::Template;

#[derive(Template)]
#[template(path = "layout.html")]
pub struct LayoutTemplate {
    pub user: String,
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub user: String,
    pub error: Option<String>,
}

#[derive(Template)]
#[template(path = "dashboard.html")]
pub struct DashboardTemplate {
    pub user: String,
    pub stats: DashboardStats,
    pub proxies: Vec<Proxy>,
    pub success_rate: String,
    pub avg_duration: String,
}

#[derive(Template)]
#[template(path = "proxy_list.html")]
pub struct ProxyListTemplate {
    pub user: String,
    pub proxies: Vec<Proxy>,
}

#[derive(Template)]
#[template(path = "proxy_form.html")]
pub struct ProxyFormTemplate {
    pub user: String,
    pub proxy: Option<Proxy>,
    pub permissions: Vec<Permission>,
}

#[derive(Template)]
#[template(path = "logs.html")]
pub struct LogsTemplate {
    pub user: String,
    pub logs: Vec<RequestLog>,
    pub filter_user_id: String,
    pub filter_proxy_id: String,
    pub filter_source_model: String,
    pub filter_is_success: String,
}
