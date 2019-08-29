use bigneon_db::dev::TestProject;
use bigneon_db::models::{OrganizationUser, Roles};

#[test]
fn create() {
    let project = TestProject::new();
    let user = project.create_user().finish();
    let user2 = project.create_user().finish();
    let organization = project
        .create_organization()
        .with_member(&user, Roles::OrgOwner)
        .finish();
    let organization_user =
        OrganizationUser::create(organization.id, user2.id, vec![Roles::OrgMember])
            .commit(project.get_connection())
            .unwrap();

    assert_eq!(organization_user.user_id, user2.id);
    assert_eq!(organization_user.organization_id, organization.id);
    assert_eq!(organization_user.role, [Roles::OrgMember]);
    assert_eq!(organization_user.id.to_string().is_empty(), false);
}

#[test]
fn event_ids() {
    let project = TestProject::new();
    let connection = project.get_connection();
    let user = project.create_user().finish();
    let organization = project.create_organization().finish();
    let event = project
        .create_event()
        .with_organization(&organization)
        .finish();
    let organization_user = organization
        .add_user(user.id, vec![Roles::Promoter], vec![event.id], connection)
        .unwrap();

    assert_eq!(
        vec![event.id],
        organization_user.event_ids(connection).unwrap()
    );
}

#[test]
fn update() {
    let project = TestProject::new();
    let user = project.create_user().finish();
    let organization = project.create_organization().finish();
    project
        .create_event()
        .with_organization(&organization)
        .finish();
    let organization_user =
        OrganizationUser::create(organization.id, user.id, vec![Roles::OrgMember])
            .commit(project.get_connection())
            .unwrap();
    let organization_user_id = organization_user.id;

    let organization_user =
        OrganizationUser::create(organization.id, user.id, vec![Roles::OrgOwner])
            .commit(project.get_connection())
            .unwrap();

    assert_eq!(vec![Roles::OrgOwner], organization_user.role);
    assert_eq!(organization_user_id, organization_user.id);
}
