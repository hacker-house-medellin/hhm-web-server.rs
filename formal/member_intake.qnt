// Finite abstraction of the authenticated H/HAUS member form lifecycle. A GET
// session authorizes private prefill/rendering, while a fresh POST session and
// exact Origin check are required before delegation or any mutation.
module member_intake {
  type Outcome = Active | AuthRejected | AuthorityUnavailable | ApiRejected

  type State = {
    outcome: Outcome,
    get_session_verified: bool,
    private_prefill_loaded: bool,
    form_rendered: bool,
    post_origin_verified: bool,
    post_session_verified: bool,
    delegated_write_token: bool,
    resume_upload_completed: bool,
    photo_upload_completed: bool,
    application_submitted: bool,
    receipt_shown: bool,
  }

  var s: State

  action init = {
    s' = {
      outcome: Active,
      get_session_verified: false,
      private_prefill_loaded: false,
      form_rendered: false,
      post_origin_verified: false,
      post_session_verified: false,
      delegated_write_token: false,
      resume_upload_completed: false,
      photo_upload_completed: false,
      application_submitted: false,
      receipt_shown: false,
    }
  }

  action verify_get_session = all {
    s.outcome == Active,
    not(s.get_session_verified),
    s' = { ...s, get_session_verified: true },
  }

  action load_private_prefill = all {
    s.outcome == Active,
    s.get_session_verified,
    not(s.private_prefill_loaded),
    s' = { ...s, private_prefill_loaded: true },
  }

  action render_form = all {
    s.outcome == Active,
    s.get_session_verified,
    s.private_prefill_loaded,
    not(s.form_rendered),
    s' = { ...s, form_rendered: true },
  }

  action verify_post_origin = all {
    s.outcome == Active,
    s.form_rendered,
    not(s.post_origin_verified),
    s' = { ...s, post_origin_verified: true },
  }

  action verify_post_session = all {
    s.outcome == Active,
    s.form_rendered,
    not(s.post_session_verified),
    s' = { ...s, post_session_verified: true },
  }

  action delegate_write_token = all {
    s.outcome == Active,
    s.post_origin_verified,
    s.post_session_verified,
    not(s.delegated_write_token),
    s' = { ...s, delegated_write_token: true },
  }

  action complete_resume_upload = all {
    s.outcome == Active,
    s.delegated_write_token,
    not(s.resume_upload_completed),
    s' = { ...s, resume_upload_completed: true },
  }

  action complete_photo_upload = all {
    s.outcome == Active,
    s.delegated_write_token,
    s.resume_upload_completed,
    not(s.photo_upload_completed),
    s' = { ...s, photo_upload_completed: true },
  }

  action submit_application = all {
    s.outcome == Active,
    s.delegated_write_token,
    s.resume_upload_completed,
    s.photo_upload_completed,
    not(s.application_submitted),
    s' = { ...s, application_submitted: true },
  }

  action show_receipt = all {
    s.outcome == Active,
    s.application_submitted,
    not(s.receipt_shown),
    s' = { ...s, receipt_shown: true },
  }

  action reject_auth = all {
    s.outcome == Active,
    not(s.application_submitted),
    s' = { ...s, outcome: AuthRejected },
  }

  action auth_authority_unavailable = all {
    s.outcome == Active,
    not(s.application_submitted),
    s' = { ...s, outcome: AuthorityUnavailable },
  }

  action api_rejects_or_cannot_confirm = all {
    s.outcome == Active,
    not(s.application_submitted),
    s' = { ...s, outcome: ApiRejected },
  }

  action exact_retry_before_receipt = all {
    s.outcome == Active,
    not(s.receipt_shown),
    s' = s,
  }

  action idle = s' = s

  action step = any {
    verify_get_session,
    load_private_prefill,
    render_form,
    verify_post_origin,
    verify_post_session,
    delegate_write_token,
    complete_resume_upload,
    complete_photo_upload,
    submit_application,
    show_receipt,
    reject_auth,
    auth_authority_unavailable,
    api_rejects_or_cannot_confirm,
    exact_retry_before_receipt,
    idle,
  }

  val private_reads_require_get_session = and {
    not(s.private_prefill_loaded) or s.get_session_verified,
    not(s.form_rendered) or and {
      s.get_session_verified,
      s.private_prefill_loaded,
    },
  }

  val mutations_require_fresh_post_authority =
    not(s.delegated_write_token) or and {
      s.post_origin_verified,
      s.post_session_verified,
      s.form_rendered,
    }

  val upload_and_application_order = and {
    not(s.resume_upload_completed) or s.delegated_write_token,
    not(s.photo_upload_completed) or and {
      s.delegated_write_token,
      s.resume_upload_completed,
    },
    not(s.application_submitted) or and {
      s.delegated_write_token,
      s.resume_upload_completed,
      s.photo_upload_completed,
    },
    not(s.receipt_shown) or s.application_submitted,
  }

  val failures_do_not_create_acceptance =
    (s.outcome != Active).implies(and {
      not(s.application_submitted),
      not(s.receipt_shown),
    })

  val member_intake_safety = and {
    private_reads_require_get_session,
    mutations_require_fresh_post_authority,
    upload_and_application_order,
    failures_do_not_create_acceptance,
  }
}
