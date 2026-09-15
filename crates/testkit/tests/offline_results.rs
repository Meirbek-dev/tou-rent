//! FR-801/802, Q-026: exercise real constraints, atomically rollback synthetic fixtures.
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn offline_closure_preserves_source_records_and_cancels_only_obsolete_duty() {
    let Some(url) = tou_testkit::database_url().expect("database configuration") else {
        return;
    };
    let db = tou_db::connect(&url).await.unwrap();
    let mut tx = db.begin().await.unwrap();
    let actor: Uuid = sqlx::query_scalar(
        "INSERT INTO core.users(email,full_name) VALUES($1::citext,'Offline fixture') RETURNING id",
    )
    .bind(format!("offline-{}@example.test", Uuid::now_v7()))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query("SELECT set_config('app.user_id',$1,true)")
        .bind(actor.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let tender: Uuid = sqlx::query_scalar("INSERT INTO core.tenders(title,organizer_id,status,opened_at,submission_deadline) VALUES('Offline fixture',$1,'qualification',core.now(),core.now()+interval '1 day') RETURNING id")
        .bind(actor).fetch_one(&mut *tx).await.unwrap();
    let object: Uuid = sqlx::query_scalar("INSERT INTO core.objects(kind,name,address,area_m2) VALUES('premises','Offline fixture','Test',10) RETURNING id")
        .fetch_one(&mut *tx).await.unwrap();
    let mut lots = Vec::new();
    let mut apps = Vec::new();
    for seq in 1..=3 {
        let lot: Uuid=sqlx::query_scalar("INSERT INTO core.lots(tender_id,seq,object_id,purpose,lease_months,base_rate_monthly,guarantee_fee,rate_calculation) VALUES($1,$2,$3,'Test',12,100,100,'{}') RETURNING id")
            .bind(tender).bind(seq).bind(object).fetch_one(&mut *tx).await.unwrap();
        lots.push(lot);
        if seq > 1 {
            let app:Uuid=sqlx::query_scalar("INSERT INTO core.applications(tender_id,lot_id,participant_id,applicant_kind,applicant_details) VALUES($1,$2,$3,'individual','{\"name\":\"Synthetic applicant\"}') RETURNING id")
                .bind(tender).bind(lot).bind(actor).fetch_one(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO core.price_proposals(application_id,amount) VALUES($1,125)")
                .bind(app)
                .execute(&mut *tx)
                .await
                .unwrap();
            apps.push(app);
        }
    }
    sqlx::query(
        "UPDATE core.tenders SET submission_deadline=core.now()-interval '1 minute' WHERE id=$1",
    )
    .bind(tender)
    .execute(&mut *tx)
    .await
    .unwrap();
    let protocol:Uuid=sqlx::query_scalar("INSERT INTO core.commission_documents(tender_id,title,number,document_date,filename,file_key,size_bytes,uploaded_by) VALUES($1,'Signed fixture','TEST',(core.now() AT TIME ZONE 'Asia/Almaty')::date,'test.pdf',$2,100,$3) RETURNING id")
        .bind(tender).bind(format!("tests/{}.pdf",Uuid::now_v7())).bind(actor).fetch_one(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO core.obligations(tender_id,rule_ref,action,assignee_role,due_at,status) VALUES($1,'test','notify_admitted','secretary',core.now()-interval '1 hour','overdue'),($1,'test','refund','secretary',core.now()-interval '1 hour','overdue')")
        .bind(tender).execute(&mut *tx).await.unwrap();
    let decisions = json!([
      {"lot_id":lots[0],"application_id":null,"resolution":"no_applications","note":"No active applications"},
      {"lot_id":lots[1],"application_id":apps[0],"resolution":"rejected","note":"Required documents absent in the signed decision"},
      {"lot_id":lots[2],"application_id":apps[1],"resolution":"single_source","note":"Recommend a single-source agreement"}
    ]);
    // Bad requests must rollback even the tender status and obligation side effects.
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM core.applications a WHERE tender_id=$1",
    )
    .bind(tender)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    // Even an unstarted auction excludes the offline path.
    sqlx::query("SAVEPOINT auction_exists")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO core.auctions(lot_id,starting_bid,bid_step) VALUES($1,125,6.25)")
        .bind(lots[2])
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(sqlx::query("INSERT INTO core.offline_tender_results(tender_id,protocol_id,recorded_by,lots) VALUES($1,$2,$3,$4)")
        .bind(tender).bind(protocol).bind(actor).bind(&decisions).execute(&mut *tx).await.is_err());
    sqlx::query("ROLLBACK TO SAVEPOINT auction_exists")
        .execute(&mut *tx)
        .await
        .unwrap();
    // Competition must not be removed or silently ignored by the import.
    sqlx::query("SAVEPOINT competition")
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query(
        "UPDATE core.tenders SET submission_deadline=core.now()+interval '1 day' WHERE id=$1",
    )
    .bind(tender)
    .execute(&mut *tx)
    .await
    .unwrap();
    let competitor: Uuid = sqlx::query_scalar(
        "INSERT INTO core.users(email,full_name) VALUES($1::citext,'Competitor') RETURNING id",
    )
    .bind(format!("competitor-{}@example.test", Uuid::now_v7()))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO core.applications(tender_id,lot_id,participant_id,applicant_kind,applicant_details) VALUES($1,$2,$3,'individual','{}')")
        .bind(tender).bind(lots[2]).bind(competitor).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "UPDATE core.tenders SET submission_deadline=core.now()-interval '1 minute' WHERE id=$1",
    )
    .bind(tender)
    .execute(&mut *tx)
    .await
    .unwrap();
    assert!(sqlx::query("INSERT INTO core.offline_tender_results(tender_id,protocol_id,recorded_by,lots) VALUES($1,$2,$3,$4)")
        .bind(tender).bind(protocol).bind(actor).bind(&decisions).execute(&mut *tx).await.is_err());
    sqlx::query("ROLLBACK TO SAVEPOINT competition")
        .execute(&mut *tx)
        .await
        .unwrap();
    for invalid in [
        json!([]),
        json!([decisions[0].clone()]),
        json!([
            decisions[0].clone(),
            decisions[0].clone(),
            decisions[2].clone()
        ]),
    ] {
        sqlx::query("SAVEPOINT invalid_request")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(sqlx::query("INSERT INTO core.offline_tender_results(tender_id,protocol_id,recorded_by,lots) VALUES($1,$2,$3,$4)")
            .bind(tender).bind(protocol).bind(actor).bind(invalid).execute(&mut *tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT invalid_request")
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO core.offline_tender_results(tender_id,protocol_id,recorded_by,lots) VALUES($1,$2,$3,$4)")
        .bind(tender).bind(protocol).bind(actor).bind(&decisions).execute(&mut *tx).await.unwrap();
    let status: String = sqlx::query_scalar("SELECT status::text FROM core.tenders WHERE id=$1")
        .bind(tender)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(status, "failed");
    let unchanged: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM core.applications WHERE tender_id=$1 AND status='submitted'",
    )
    .bind(tender)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(unchanged, 2);
    let after: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM core.applications a WHERE tender_id=$1",
    )
    .bind(tender)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(before, after, "all application fields remain unchanged");
    let duty: String = sqlx::query_scalar(
        "SELECT status::text FROM core.obligations WHERE tender_id=$1 AND action='notify_admitted'",
    )
    .bind(tender)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(duty, "cancelled");
    let refund: String = sqlx::query_scalar(
        "SELECT status::text FROM core.obligations WHERE tender_id=$1 AND action='refund'",
    )
    .bind(tender)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(refund, "overdue");
    let snapshot: serde_json::Value =
        sqlx::query_scalar("SELECT lots FROM core.offline_tender_results WHERE tender_id=$1")
            .bind(tender)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(snapshot[2]["price"], "125.00");
    assert_eq!(snapshot[0]["ground"], "no_applications");
    assert_eq!(snapshot[1]["ground"], "single_application");
    let audit:i64=sqlx::query_scalar("SELECT count(*) FROM audit.log WHERE table_name='core.offline_tender_results' AND actor_id=$1")
        .bind(actor).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(audit, 1);
    // Immutable result and source statuses; no reopening or second closure.
    for statement in [
        "UPDATE core.offline_tender_results SET lots='[]' WHERE tender_id=$1",
        "UPDATE core.tenders SET status='repeat_announced' WHERE id=$1",
        "UPDATE core.applications SET status='rejected' WHERE tender_id=$1",
        "DELETE FROM core.applications WHERE tender_id=$1",
        "UPDATE core.obligations SET status='done' WHERE tender_id=$1 AND action='notify_admitted'",
        "INSERT INTO core.offline_tender_results(tender_id,protocol_id,recorded_by,lots) SELECT tender_id,protocol_id,recorded_by,lots FROM core.offline_tender_results WHERE tender_id=$1",
    ] {
        sqlx::query("SAVEPOINT immutable")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(
            sqlx::query(statement)
                .bind(tender)
                .execute(&mut *tx)
                .await
                .is_err()
        );
        sqlx::query("ROLLBACK TO SAVEPOINT immutable")
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.rollback().await.unwrap();
}
