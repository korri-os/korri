# Match the terminal app.session.status replies produced by korrid.
._tag == "app.session.status"
and .outcome._tag == "Err"
and (.outcome.payload.code == "NoActiveSession"
     or .outcome.payload.code == "SessionCompleted")
