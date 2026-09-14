// A data-dependent `before:mutation` rule (#1328).
//
// The rule "an order may not push a customer past their credit limit" cannot be
// written from the mutation's input alone: the limit and the balance live in the
// database. Before #1328 a before-hook ran on a `NoopHostContext` through the
// sync dispatch path, so `fraiseql_query` failed loud and this function could not
// exist — the rule had to go into the SQL mutation function or an
// `after:mutation` compensation.
//
// The guest argument is the mutation's **resolved arguments** — request variables
// merged with inline literals — so the amount is `args.input.amount`.
//
// The read is a *fast, friendly rejection*, not the authoritative rule: it runs
// outside the mutation's transaction, so a concurrent order can still push the
// customer over. The authoritative check stays a constraint or the SQL function.
// See `docs/architecture/functions.md`.
export default async (args: { input: { customer_id: string; amount: number } }) => {
  const { customer_id, amount } = args.input;

  const raw = await Deno.core.ops.fraiseql_query(
    `query { customer(id: "${customer_id}") { creditLimit outstanding } }`,
    "{}",
  );
  const customer = JSON.parse(raw)?.data?.customer;
  if (!customer) {
    return { abort: `customer ${customer_id} not found` };
  }

  const remaining = customer.creditLimit - customer.outstanding;
  if (amount > remaining) {
    Deno.core.ops.fraiseql_log(2, `refusing ${amount}: only ${remaining} left`);
    return { abort: `amount ${amount} exceeds the remaining credit of ${remaining}` };
  }

  // Under the limit: proceed, stamping the check onto the input the write binds
  // from. Returning `{ input }` is how a hook rewrites arguments.
  return { input: { ...args.input, credit_checked: true } };
};
