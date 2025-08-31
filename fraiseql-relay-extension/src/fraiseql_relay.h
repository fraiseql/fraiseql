/*
 * fraiseql_relay.h
 *
 * Header file for FraiseQL Relay PostgreSQL extension
 * High-performance C implementation of critical path functions
 */

#ifndef FRAISEQL_RELAY_H
#define FRAISEQL_RELAY_H

#include "postgres.h"
#include "fmgr.h"
#include "utils/uuid.h"
#include "utils/jsonb.h"
#include "utils/builtins.h"
#include "executor/spi.h"
#include "utils/memutils.h"
#include "catalog/pg_type.h"

/* Version information */
#define FRAISEQL_RELAY_VERSION "1.0"
#define FRAISEQL_RELAY_VERSION_NUM 10000

/* Function declarations for performance-critical operations */
PG_FUNCTION_INFO_V1(fraiseql_resolve_node_fast);
PG_FUNCTION_INFO_V1(fraiseql_resolve_nodes_batch);
PG_FUNCTION_INFO_V1(fraiseql_encode_global_id);
PG_FUNCTION_INFO_V1(fraiseql_decode_global_id);
PG_FUNCTION_INFO_V1(fraiseql_refresh_nodes_view_fast);

/* Structure for node resolution result */
typedef struct NodeResult {
    char *typename;
    Jsonb *data;
    char *entity_name;
    char *source_used;
} NodeResult;

/* Structure for batch resolution */
typedef struct BatchNodeResult {
    int count;
    NodeResult *nodes;
} BatchNodeResult;

/* Utility functions */
static Datum build_node_result_tuple(FunctionCallInfo fcinfo, NodeResult *result);
static NodeResult* resolve_single_node(pg_uuid_t *node_id);
static BatchNodeResult* resolve_multiple_nodes(pg_uuid_t *node_ids, int count);
static void cleanup_node_result(NodeResult *result);
static void cleanup_batch_result(BatchNodeResult *result);

/* Error handling */
#define FRAISEQL_ERROR(msg) \
    ereport(ERROR, \
            (errcode(ERRCODE_INTERNAL_ERROR), \
             errmsg("FraiseQL Relay: %s", msg)))

#define FRAISEQL_WARNING(msg) \
    ereport(WARNING, \
            (errmsg("FraiseQL Relay: %s", msg)))

#define FRAISEQL_NOTICE(msg) \
    ereport(NOTICE, \
            (errmsg("FraiseQL Relay: %s", msg)))

/* Memory context for extension operations */
extern MemoryContext fraiseql_relay_context;

#endif /* FRAISEQL_RELAY_H */
