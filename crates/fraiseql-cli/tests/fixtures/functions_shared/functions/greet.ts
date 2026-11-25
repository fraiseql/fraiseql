// `import type` is erased before the function runs, so `types.ts` is neither loaded nor
// hashed with it.
import type { Person } from "@shared/types.ts";
import { greeting } from "@shared/text.ts";

const person: Person = { name: "fraiseql" };

export default () => greeting(person.name);
