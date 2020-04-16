use crate::functional::base;
use db::prelude::*;

#[cfg(test)]
mod sales_summary_report_tests {
    use super::*;
    #[actix_rt::test]
    async fn sales_summary_report_org_member() {
        base::admin::reports::sales_summary_report(Roles::OrgMember, true).await;
    }
    #[actix_rt::test]
    async fn sales_summary_report_admin() {
        base::admin::reports::sales_summary_report(Roles::Admin, true).await;
    }
    #[actix_rt::test]
    async fn sales_summary_report_super() {
        base::admin::reports::sales_summary_report(Roles::Super, true).await;
    }
    #[actix_rt::test]
    async fn sales_summary_report_user() {
        base::admin::reports::sales_summary_report(Roles::User, false).await;
    }
    #[actix_rt::test]
    async fn sales_summary_report_org_owner() {
        base::admin::reports::sales_summary_report(Roles::OrgOwner, true).await;
    }
    #[actix_rt::test]
    async fn sales_summary_report_door_person() {
        base::admin::reports::sales_summary_report(Roles::DoorPerson, false).await;
    }
    #[actix_rt::test]
    async fn sales_summary_report_promoter() {
        base::admin::reports::sales_summary_report(Roles::Promoter, false).await;
    }
    #[actix_rt::test]
    async fn sales_summary_report_promoter_read_only() {
        base::admin::reports::sales_summary_report(Roles::PromoterReadOnly, false).await;
    }
    #[actix_rt::test]
    async fn sales_summary_report_org_admin() {
        base::admin::reports::sales_summary_report(Roles::OrgAdmin, true).await;
    }
    #[actix_rt::test]
    async fn sales_summary_report_box_office() {
        base::admin::reports::sales_summary_report(Roles::OrgBoxOffice, false).await;
    }
}
