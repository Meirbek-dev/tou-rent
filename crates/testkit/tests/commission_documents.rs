//! FR-703/FR-1601/FR-1602: offline documents do not alter electronic outcomes.
use tou_db::commission_documents::{self as documents, NewDocument};
use uuid::Uuid;

#[tokio::test]
async fn offline_documents_enforce_recipient_visibility_and_preserve_outcomes() {
    let Some(url) = tou_testkit::database_url().expect("database configuration") else {
        return;
    };
    let pool = tou_db::connect(&url).await.expect("database");
    let mut users = Vec::new();
    for _ in 0..3 {
        let id: Uuid = sqlx::query_scalar("INSERT INTO core.users(email,full_name) VALUES($1::citext,'Document test') RETURNING id")
            .bind(format!("document-{}@example.test", Uuid::now_v7())).fetch_one(&pool).await.unwrap();
        users.push(id);
    }
    let actor = users[0];
    let owner = users[1];
    let other = users[2];
    let tender: Uuid = sqlx::query_scalar("INSERT INTO core.tenders(title,organizer_id,status,opened_at) VALUES('Document test',$1,'qualification',core.now()) RETURNING id")
        .bind(actor).fetch_one(&pool).await.unwrap();
    let closed: Uuid = sqlx::query_scalar(
        "INSERT INTO core.tenders(title,organizer_id) VALUES('Unopened test',$1) RETURNING id",
    )
    .bind(actor)
    .fetch_one(&pool)
    .await
    .unwrap();
    let object: Uuid = sqlx::query_scalar("INSERT INTO core.objects(kind,name,address,area_m2) VALUES('premises','Test','Test',10) RETURNING id")
        .fetch_one(&pool).await.unwrap();
    let lot: Uuid = sqlx::query_scalar("INSERT INTO core.lots(tender_id,seq,object_id,purpose,lease_months,base_rate_monthly,guarantee_fee,rate_calculation) VALUES($1,1,$2,'Test',12,100,100,'{}') RETURNING id")
        .bind(tender).bind(object).fetch_one(&pool).await.unwrap();
    let mut apps = Vec::new();
    for participant in [owner, other] {
        let app: Uuid = sqlx::query_scalar("INSERT INTO core.applications(tender_id,lot_id,participant_id,applicant_kind,applicant_details) VALUES($1,$2,$3,'individual','{}') RETURNING id")
            .bind(tender).bind(lot).bind(participant).fetch_one(&pool).await.unwrap();
        apps.push(app);
    }
    assert!(!documents::valid_target(&pool, closed, None).await.unwrap());
    assert!(
        !documents::valid_target(&pool, closed, Some(apps[0]))
            .await
            .unwrap()
    );
    assert!(
        documents::valid_target(&pool, tender, Some(apps[0]))
            .await
            .unwrap()
    );
    let mut ids = Vec::new();
    for application_id in [None, Some(apps[0])] {
        let id = Uuid::now_v7();
        documents::insert(
            &pool,
            actor,
            NewDocument {
                id,
                tender_id: tender,
                application_id,
                title: "Протокол",
                number: "1",
                document_date: time::macros::date!(2026 - 09 - 11),
                filename: "Протокол.pdf",
                file_key: &format!("tests/{id}.pdf"),
                size_bytes: 10,
            },
        )
        .await
        .unwrap();
        assert!(
            documents::visible(&pool, id, owner, false)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            documents::visible(&pool, id, actor, true)
                .await
                .unwrap()
                .is_some()
        );
        documents::share(&pool, actor, id, true).await.unwrap();
        ids.push(id);
    }
    assert!(
        documents::visible(&pool, ids[0], other, false)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        documents::visible(&pool, ids[1], other, false)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        documents::visible(&pool, ids[0], actor, false)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        documents::visible(&pool, ids[1], owner, false)
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        documents::list(&pool, owner, false, Some(tender))
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        documents::list(&pool, other, false, Some(tender))
            .await
            .unwrap()
            .len(),
        1
    );
    documents::share(&pool, actor, ids[1], false).await.unwrap();
    assert!(
        documents::visible(&pool, ids[1], owner, false)
            .await
            .unwrap()
            .is_none()
    );
    let replace =
        sqlx::query("UPDATE core.commission_documents SET file_key='replacement' WHERE id=$1")
            .bind(ids[0])
            .execute(&pool)
            .await;
    assert!(replace.is_err(), "stored original cannot be replaced");
    let state: String =
        sqlx::query_scalar("SELECT status::text FROM core.applications WHERE id=$1")
            .bind(apps[0])
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "submitted");
    let dossier: i64 = sqlx::query_scalar("SELECT count(*) FROM core.dossier_items WHERE source_table='core.commission_documents' AND tender_id=$1 AND file_key IS NOT NULL").bind(tender).fetch_one(&pool).await.unwrap();
    assert_eq!(dossier, 2);
    let audited: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit.log WHERE table_name='core.commission_documents' AND row_id=$1",
    )
    .bind(ids[1])
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(audited >= 3, "upload, release and hide are audited");

    use tou_db::purge::{PurgeScope, purge};
    purge(&pool, actor, PurgeScope::Applications, Some(&[apps[0]]))
        .await
        .unwrap();
    assert!(
        documents::visible(&pool, ids[1], actor, true)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        documents::visible(&pool, ids[0], actor, true)
            .await
            .unwrap()
            .is_some()
    );
    purge(&pool, actor, PurgeScope::Tenders, Some(&[tender, closed]))
        .await
        .unwrap();
    assert!(
        documents::visible(&pool, ids[0], actor, true)
            .await
            .unwrap()
            .is_none()
    );
    purge(&pool, actor, PurgeScope::Objects, Some(&[object]))
        .await
        .unwrap();
}
