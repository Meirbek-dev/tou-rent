//! Q-027: административная корректировка хранит снимок, но не имитирует процедуру.

use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn admin_outcome_changes_only_tender_and_preserves_evidence() {
    let Some(url) = tou_testkit::database_url().expect("database configuration") else {
        return;
    };
    let db = tou_db::connect(&url).await.unwrap();
    let mut tx = db.begin().await.unwrap();
    let actor: Uuid = sqlx::query_scalar(
        "INSERT INTO core.users(email,full_name) VALUES($1::citext,'Admin outcome fixture') RETURNING id",
    )
    .bind(format!("admin-outcome-{}@example.test", Uuid::now_v7()))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query("SELECT set_config('app.user_id',$1,true)")
        .bind(actor.to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let tender: Uuid = sqlx::query_scalar(
        "INSERT INTO core.tenders(title,organizer_id,status,opened_at,submission_deadline,failure_ground,consequence,failed_at) \
         VALUES('Administrative outcome',$1,'failed',core.now(),core.now()+interval '1 day','single_application','repeat',core.now()) RETURNING id",
    )
    .bind(actor)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let object: Uuid = sqlx::query_scalar(
        "INSERT INTO core.objects(kind,name,address,area_m2) VALUES('premises','Outcome fixture','Test',10) RETURNING id",
    )
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let mut decisions = Vec::new();
    let mut applications = Vec::new();
    for seq in 1..=2 {
        let lot: Uuid = sqlx::query_scalar(
            "INSERT INTO core.lots(tender_id,seq,object_id,purpose,lease_months,base_rate_monthly,guarantee_fee,rate_calculation) \
             VALUES($1,$2,$3,'Test',12,100,100,'{}') RETURNING id",
        )
        .bind(tender)
        .bind(seq)
        .bind(object)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        let application: Uuid = sqlx::query_scalar(
            "INSERT INTO core.applications(tender_id,lot_id,participant_id,applicant_kind,applicant_details) \
             VALUES($1,$2,$3,'individual','{\"name\":\"Offline winner\"}') RETURNING id",
        )
        .bind(tender)
        .bind(lot)
        .bind(actor)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        sqlx::query("INSERT INTO core.price_proposals(application_id,amount) VALUES($1,125)")
            .bind(application)
            .execute(&mut *tx)
            .await
            .unwrap();
        applications.push(application);
        decisions.push(json!({"lot_id":lot,"application_id":application,"price":"125.00"}));
    }
    sqlx::query(
        "UPDATE core.tenders SET submission_deadline=core.now()-interval '1 day' WHERE id=$1",
    )
    .bind(tender)
    .execute(&mut *tx)
    .await
    .unwrap();
    let document: Uuid = sqlx::query_scalar(
        "INSERT INTO core.commission_documents(tender_id,title,number,document_date,filename,file_key,size_bytes,uploaded_by) \
         VALUES($1,'Signed result','OUTCOME',(core.now() AT TIME ZONE 'Asia/Almaty')::date,'outcome.pdf',$2,100,$3) RETURNING id",
    )
    .bind(tender)
    .bind(format!("tests/{}.pdf", Uuid::now_v7()))
    .bind(actor)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let failed_protocol: Uuid = sqlx::query_scalar(
        "INSERT INTO core.protocols(tender_id,kind,content) VALUES($1,'failed','{}') RETURNING id",
    )
    .bind(tender)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let notifications_before: i64 = sqlx::query_scalar("SELECT count(*) FROM core.notifications")
        .fetch_one(&mut *tx)
        .await
        .unwrap();

    sqlx::query(
        "UPDATE core.tenders SET status='summed_up',failure_ground=NULL,consequence=NULL,failed_at=NULL, \
         admin_outcome_protocol_id=$2,admin_outcome_lots=$3,admin_outcome_reason='Signed offline decision', \
         admin_outcome_recorded_by=$4,admin_outcome_recorded_at=core.now() WHERE id=$1",
    )
    .bind(tender)
    .bind(document)
    .bind(json!(decisions))
    .bind(actor)
    .execute(&mut *tx)
    .await
    .unwrap();

    let stored: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_build_object('status',status::text,'failure_ground',failure_ground, \
         'protocol',admin_outcome_protocol_id,'lots',admin_outcome_lots) FROM core.tenders WHERE id=$1",
    )
    .bind(tender)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(stored["status"], "summed_up");
    assert!(stored["failure_ground"].is_null());
    assert_eq!(stored["protocol"], document.to_string());
    assert_eq!(stored["lots"].as_array().map(Vec::len), Some(2));

    let unchanged_apps: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM core.applications WHERE id=ANY($1) AND status='submitted'",
    )
    .bind(&applications)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(unchanged_apps, 2);
    let old_protocol_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM core.protocols WHERE id=$1 AND kind='failed')",
    )
    .bind(failed_protocol)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(old_protocol_exists);
    let contracts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM core.contracts WHERE tender_id=$1")
            .bind(tender)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(contracts, 0);
    let notifications_after: i64 = sqlx::query_scalar("SELECT count(*) FROM core.notifications")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(notifications_after, notifications_before);
    let audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit.log WHERE table_name='core.tenders' AND row_id=$1 AND actor_id=$2",
    )
    .bind(tender)
    .bind(actor)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(audit > 0);
    assert!(
        sqlx::query("UPDATE core.tenders SET admin_outcome_reason='Replacement' WHERE id=$1")
            .bind(tender)
            .execute(&mut *tx)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}
