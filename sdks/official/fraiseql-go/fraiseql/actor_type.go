package fraiseql

import (
	"fmt"
	"strings"
)

// ActorTypes is the roster #966's actor gate is an allow-list of.
//
// snake_case, as the compiler spells it
// (crates/fraiseql-core/src/security/actor_type.rs) and as the change-log
// actor_type TEXT column stores it.
var ActorTypes = []string{"human_user", "service_account", "ai_agent", "system_job"}

const (
	ActorHumanUser      = "human_user"
	ActorServiceAccount = "service_account"
	ActorAIAgent        = "ai_agent"
	ActorSystemJob      = "system_job"
)

// validateRequiresActor checks an allow-list where the author wrote it.
//
// The compiler refuses an unknown token by name, but only at compile time, and this is a
// security gate enforced in the same executor arm as requires_role on every transport —
// one that fails late fails after the author has stopped looking (#1123).
//
// An empty list is refused rather than passed on: the compiled schema omits the key when
// empty, so an empty allow-list reads as a declared gate and compiles to none at all.
func validateRequiresActor(operationName string, actors []string) error {
	if len(actors) == 0 {
		return fmt.Errorf(
			"%s: RequiresActor was given an empty list. An empty allow-list admits nobody "+
				"and is dropped from the compiled schema, which admits everybody — name the "+
				"actor types instead. Valid: %s",
			operationName, strings.Join(ActorTypes, ", "))
	}
	var unknown []string
	for _, a := range actors {
		known := false
		for _, valid := range ActorTypes {
			if a == valid {
				known = true
				break
			}
		}
		if !known {
			unknown = append(unknown, a)
		}
	}
	if len(unknown) > 0 {
		return fmt.Errorf("%s: RequiresActor names unknown actor type(s) %s. Valid: %s",
			operationName, strings.Join(unknown, ", "), strings.Join(ActorTypes, ", "))
	}
	return nil
}
