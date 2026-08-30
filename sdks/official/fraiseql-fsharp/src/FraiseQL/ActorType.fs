namespace FraiseQL

/// The actor roster #966's actor gate is an allow-list of.
///
/// snake_case, as the compiler spells it
/// (`crates/fraiseql-core/src/security/actor_type.rs`) and as the change-log
/// `actor_type TEXT` column stores it.
module ActorType =

    /// A human end user — the default classification for an ordinary user JWT.
    let humanUser = "human_user"

    /// A non-human service account: an API key, or a token carrying the scope.
    let serviceAccount = "service_account"

    /// An autonomous agent acting for a user, via an RFC 8693 `act` claim.
    let aiAgent = "ai_agent"

    /// An internal scheduled or system-triggered job. Never token-derived.
    let systemJob = "system_job"

    /// Every valid token, in declaration order.
    let all = [ humanUser; serviceAccount; aiAgent; systemJob ]

    /// Checks an allow-list where the author wrote it.
    ///
    /// The compiler refuses an unknown token by name, but only at compile time, and this
    /// is a security gate enforced in the same executor arm as `requires_role` on every
    /// transport — one that fails late fails after the author has stopped looking (#1123).
    ///
    /// An empty list is refused rather than passed on: the compiled schema omits the key
    /// when empty, so an empty allow-list reads as a declared gate and compiles to none.
    let validate (operationName: string) (actors: string list) : unit =
        if List.isEmpty actors then
            failwithf
                "%s: requiresActor was given an empty list. An empty allow-list admits nobody and is dropped from the compiled schema, which admits everybody - name the actor types instead. Valid: %s"
                operationName
                (String.concat ", " all)

        match actors |> List.filter (fun a -> not (List.contains a all)) with
        | [] -> ()
        | unknown ->
            failwithf
                "%s: requiresActor names unknown actor type(s) %s. Valid: %s"
                operationName
                (String.concat ", " unknown)
                (String.concat ", " all)
