/*
 * fraiseql_relay.c
 *
 * High-performance C implementation of FraiseQL Relay GraphQL functions
 *
 * This module provides optimized implementations of performance-critical
 * operations for GraphQL Relay specification compliance.
 */

#include "fraiseql_relay.h"
#include "utils/lsyscache.h"
#include "utils/syscache.h"
#include "access/htup_details.h"
#include "funcapi.h"
#include "miscadmin.h"

PG_MODULE_MAGIC;

/* Memory context for extension operations */
MemoryContext fraiseql_relay_context = NULL;

/*
 * Module initialization
 */
void
_PG_init(void)
{
    /* Create persistent memory context for the extension */
    fraiseql_relay_context = AllocSetContextCreate(TopMemoryContext,
                                                   "FraiseQL Relay Context",
                                                   ALLOCSET_DEFAULT_SIZES);

    elog(NOTICE, "FraiseQL Relay extension v%s initialized", FRAISEQL_RELAY_VERSION);
}

/*
 * Module cleanup
 */
void
_PG_fini(void)
{
    if (fraiseql_relay_context)
    {
        MemoryContextDelete(fraiseql_relay_context);
        fraiseql_relay_context = NULL;
    }
}

/*
 * fraiseql_resolve_node_fast
 *
 * High-performance node resolution by UUID
 * Optimized C implementation of core.resolve_node()
 */
Datum
fraiseql_resolve_node_fast(PG_FUNCTION_ARGS)
{
    pg_uuid_t  *node_id;
    ReturnSetInfo *rsinfo = (ReturnSetInfo *) fcinfo->resultinfo;
    TupleDesc   tupdesc;
    Tuplestorestate *tupstore;
    HeapTuple   tuple;
    MemoryContext per_query_ctx;
    MemoryContext oldcontext;

    /* Input validation */
    if (PG_ARGISNULL(0))
        PG_RETURN_NULL();

    node_id = PG_GETARG_UUID_P(0);

    /* Setup for returning set of records */
    if (rsinfo == NULL || !IsA(rsinfo, ReturnSetInfo))
        ereport(ERROR,
                (errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
                 errmsg("set-valued function called in context that cannot accept a set")));

    if (!(rsinfo->allowedModes & SFRM_Materialize))
        ereport(ERROR,
                (errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
                 errmsg("materialize mode required, but it is not allowed in this context")));

    /* Build tuple descriptor for return type */
    if (get_call_result_type(fcinfo, NULL, &tupdesc) != TYPEFUNC_COMPOSITE)
        elog(ERROR, "return type must be a row type");

    per_query_ctx = rsinfo->econtext->ecxt_per_query_memory;
    oldcontext = MemoryContextSwitchTo(per_query_ctx);

    tupstore = tuplestore_begin_heap(true, false, work_mem);
    rsinfo->returnMode = SFRM_Materialize;
    rsinfo->setResult = tupstore;
    rsinfo->setDesc = tupdesc;

    MemoryContextSwitchTo(oldcontext);

    /* Execute optimized node resolution */
    if (SPI_connect() != SPI_OK_CONNECT)
        elog(ERROR, "SPI_connect failed");

    /* Use parameterized query for better performance and security */
    {
        const char *query = "SELECT __typename, data, entity_name FROM core.v_nodes WHERE id = $1 LIMIT 1";
        Oid argtypes[1] = { UUIDOID };
        Datum values[1];
        char nulls[1] = { ' ' };

        values[0] = UUIDPGetDatum(node_id);

        if (SPI_execute_with_args(query, 1, argtypes, values, nulls, true, 1) != SPI_OK_SELECT)
            elog(ERROR, "SPI_execute_with_args failed");

        if (SPI_processed > 0)
        {
            HeapTuple spi_tuple = SPI_tuptable->vals[0];
            TupleDesc spi_tupdesc = SPI_tuptable->tupdesc;

            Datum result_values[4]; /* __typename, data, entity_name, source_used */
            bool result_nulls[4] = { false, false, false, false };

            /* Extract values from SPI result */
            bool isnull;

            result_values[0] = SPI_getbinval(spi_tuple, spi_tupdesc, 1, &isnull);
            result_nulls[0] = isnull;

            result_values[1] = SPI_getbinval(spi_tuple, spi_tupdesc, 2, &isnull);
            result_nulls[1] = isnull;

            result_values[2] = SPI_getbinval(spi_tuple, spi_tupdesc, 3, &isnull);
            result_nulls[2] = isnull;

            /* source_used - for now, just hardcode 'v_nodes' */
            result_values[3] = CStringGetTextDatum("v_nodes");
            result_nulls[3] = false;

            /* Build result tuple */
            tuple = heap_form_tuple(tupdesc, result_values, result_nulls);

            oldcontext = MemoryContextSwitchTo(per_query_ctx);
            tuplestore_puttuple(tupstore, tuple);
            MemoryContextSwitchTo(oldcontext);

            heap_freetuple(tuple);
        }
    }

    SPI_finish();

    /* Clean up and return */
    tuplestore_donestoring(tupstore);

    return (Datum) 0;
}

/*
 * fraiseql_resolve_nodes_batch
 *
 * Batch resolution of multiple nodes for performance
 * Takes array of UUIDs, returns array of results
 */
Datum
fraiseql_resolve_nodes_batch(PG_FUNCTION_ARGS)
{
    ArrayType  *node_ids_array;
    ReturnSetInfo *rsinfo = (ReturnSetInfo *) fcinfo->resultinfo;
    TupleDesc   tupdesc;
    Tuplestorestate *tupstore;
    MemoryContext per_query_ctx;
    MemoryContext oldcontext;

    /* Input validation */
    if (PG_ARGISNULL(0))
        PG_RETURN_NULL();

    node_ids_array = PG_GETARG_ARRAYTYPE_P(0);

    /* Validate array type */
    if (ARR_ELEMTYPE(node_ids_array) != UUIDOID)
        ereport(ERROR,
                (errcode(ERRCODE_DATATYPE_MISMATCH),
                 errmsg("array must contain UUID elements")));

    /* Setup for returning set */
    if (rsinfo == NULL || !IsA(rsinfo, ReturnSetInfo))
        ereport(ERROR,
                (errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
                 errmsg("set-valued function called in context that cannot accept a set")));

    if (!(rsinfo->allowedModes & SFRM_Materialize))
        ereport(ERROR,
                (errcode(ERRCODE_FEATURE_NOT_SUPPORTED),
                 errmsg("materialize mode required, but it is not allowed in this context")));

    if (get_call_result_type(fcinfo, NULL, &tupdesc) != TYPEFUNC_COMPOSITE)
        elog(ERROR, "return type must be a row type");

    per_query_ctx = rsinfo->econtext->ecxt_per_query_memory;
    oldcontext = MemoryContextSwitchTo(per_query_ctx);

    tupstore = tuplestore_begin_heap(true, false, work_mem);
    rsinfo->returnMode = SFRM_Materialize;
    rsinfo->setResult = tupstore;
    rsinfo->setDesc = tupdesc;

    MemoryContextSwitchTo(oldcontext);

    /* Process array elements */
    {
        int16       typlen;
        bool        typbyval;
        char        typalign;
        Datum      *elems;
        bool       *nulls;
        int         nelems;
        int         i;

        get_typlenbyvalalign(UUIDOID, &typlen, &typbyval, &typalign);

        deconstruct_array(node_ids_array, UUIDOID, typlen, typbyval, typalign,
                         &elems, &nulls, &nelems);

        if (SPI_connect() != SPI_OK_CONNECT)
            elog(ERROR, "SPI_connect failed");

        /* Build batch query using ANY() for better performance */
        {
            StringInfoData query_buf;
            char *id_list;
            StringInfoData ids_buf;

            initStringInfo(&query_buf);
            initStringInfo(&ids_buf);

            /* Build array literal for ANY() clause */
            appendStringInfoString(&ids_buf, "ARRAY[");
            for (i = 0; i < nelems; i++)
            {
                if (nulls[i])
                    continue;

                if (i > 0)
                    appendStringInfoString(&ids_buf, ",");

                appendStringInfo(&ids_buf, "'%s'",
                               DatumGetCString(DirectFunctionCall1(uuid_out, elems[i])));
            }
            appendStringInfoString(&ids_buf, "]::uuid[]");

            appendStringInfo(&query_buf,
                           "SELECT __typename, data, entity_name, id "
                           "FROM core.v_nodes "
                           "WHERE id = ANY(%s) "
                           "ORDER BY __typename, id",
                           ids_buf.data);

            if (SPI_execute(query_buf.data, true, 0) != SPI_OK_SELECT)
                elog(ERROR, "batch query failed: %s", query_buf.data);

            /* Process results */
            for (i = 0; i < SPI_processed; i++)
            {
                HeapTuple spi_tuple = SPI_tuptable->vals[i];
                TupleDesc spi_tupdesc = SPI_tuptable->tupdesc;

                Datum result_values[4];
                bool result_nulls[4] = { false, false, false, false };
                bool isnull;

                result_values[0] = SPI_getbinval(spi_tuple, spi_tupdesc, 1, &isnull);
                result_nulls[0] = isnull;

                result_values[1] = SPI_getbinval(spi_tuple, spi_tupdesc, 2, &isnull);
                result_nulls[1] = isnull;

                result_values[2] = SPI_getbinval(spi_tuple, spi_tupdesc, 3, &isnull);
                result_nulls[2] = isnull;

                result_values[3] = CStringGetTextDatum("v_nodes_batch");
                result_nulls[3] = false;

                HeapTuple tuple = heap_form_tuple(tupdesc, result_values, result_nulls);

                oldcontext = MemoryContextSwitchTo(per_query_ctx);
                tuplestore_puttuple(tupstore, tuple);
                MemoryContextSwitchTo(oldcontext);

                heap_freetuple(tuple);
            }

            pfree(query_buf.data);
            pfree(ids_buf.data);
        }

        SPI_finish();
        pfree(elems);
        pfree(nulls);
    }

    tuplestore_donestoring(tupstore);
    return (Datum) 0;
}

/*
 * fraiseql_encode_global_id
 *
 * Encode typename + UUID as base64 global ID (Relay standard)
 */
Datum
fraiseql_encode_global_id(PG_FUNCTION_ARGS)
{
    text       *typename_text;
    pg_uuid_t  *local_id;
    char       *typename;
    char       *uuid_str;
    StringInfoData buf;
    char       *encoded;
    text       *result;

    if (PG_ARGISNULL(0) || PG_ARGISNULL(1))
        PG_RETURN_NULL();

    typename_text = PG_GETARG_TEXT_PP(0);
    local_id = PG_GETARG_UUID_P(1);

    typename = text_to_cstring(typename_text);
    uuid_str = DatumGetCString(DirectFunctionCall1(uuid_out, UUIDPGetDatum(local_id)));

    /* Build the composite string: "typename:uuid" */
    initStringInfo(&buf);
    appendStringInfo(&buf, "%s:%s", typename, uuid_str);

    /* Base64 encode */
    encoded = DatumGetCString(DirectFunctionCall1(encode,
                             PointerGetDatum(cstring_to_text(buf.data))));

    result = cstring_to_text(encoded);

    pfree(typename);
    pfree(uuid_str);
    pfree(buf.data);
    pfree(encoded);

    PG_RETURN_TEXT_P(result);
}

/*
 * fraiseql_decode_global_id
 *
 * Decode base64 global ID back to typename + UUID
 * Returns composite type (typename text, local_id uuid)
 */
Datum
fraiseql_decode_global_id(PG_FUNCTION_ARGS)
{
    text       *global_id_text;
    char       *global_id;
    char       *decoded;
    char       *colon_pos;
    char       *typename;
    char       *uuid_str;
    TupleDesc   tupdesc;
    HeapTuple   tuple;
    Datum       values[2];
    bool        nulls[2] = { false, false };

    if (PG_ARGISNULL(0))
        PG_RETURN_NULL();

    global_id_text = PG_GETARG_TEXT_PP(0);
    global_id = text_to_cstring(global_id_text);

    /* Base64 decode */
    decoded = DatumGetCString(DirectFunctionCall1(decode,
                             PointerGetDatum(cstring_to_text(global_id))));

    /* Find the colon separator */
    colon_pos = strchr(decoded, ':');
    if (colon_pos == NULL)
        ereport(ERROR,
                (errcode(ERRCODE_INVALID_PARAMETER_VALUE),
                 errmsg("invalid global ID format: missing colon separator")));

    /* Split into typename and UUID */
    *colon_pos = '\0';
    typename = decoded;
    uuid_str = colon_pos + 1;

    /* Build return tuple */
    if (get_call_result_type(fcinfo, NULL, &tupdesc) != TYPEFUNC_COMPOSITE)
        elog(ERROR, "return type must be a row type");

    values[0] = CStringGetTextDatum(typename);
    values[1] = DirectFunctionCall1(uuid_in, CStringGetDatum(uuid_str));

    tuple = heap_form_tuple(tupdesc, values, nulls);

    pfree(global_id);
    pfree(decoded);

    PG_RETURN_DATUM(HeapTupleGetDatum(tuple));
}

/*
 * fraiseql_refresh_nodes_view_fast
 *
 * Optimized C implementation of view refresh
 * Reduces overhead compared to PL/pgSQL version
 */
Datum
fraiseql_refresh_nodes_view_fast(PG_FUNCTION_ARGS)
{
    StringInfoData view_sql;
    StringInfoData union_part;
    int entity_count = 0;

    if (SPI_connect() != SPI_OK_CONNECT)
        elog(ERROR, "SPI_connect failed");

    initStringInfo(&view_sql);
    initStringInfo(&union_part);

    /* Query registered entities */
    if (SPI_execute("SELECT entity_name, graphql_type, pk_column, "
                   "COALESCE(tv_table, v_table) as data_table, "
                   "source_table, COALESCE(soft_delete_column, 'deleted_at') as delete_col "
                   "FROM core.tb_entity_registry "
                   "WHERE v_table IS NOT NULL "
                   "ORDER BY entity_name",
                   true, 0) != SPI_OK_SELECT)
        elog(ERROR, "failed to query entity registry");

    entity_count = SPI_processed;

    if (entity_count == 0)
    {
        /* No entities - create empty view */
        appendStringInfoString(&view_sql,
            "CREATE OR REPLACE VIEW core.v_nodes AS "
            "SELECT NULL::UUID as id, NULL::TEXT as __typename, "
            "NULL::TEXT as entity_name, NULL::TEXT as source_table, "
            "NULL::JSONB as data, NULL::TIMESTAMPTZ as created_at, "
            "NULL::TIMESTAMPTZ as updated_at WHERE FALSE");
    }
    else
    {
        appendStringInfoString(&view_sql, "CREATE OR REPLACE VIEW core.v_nodes AS ");

        for (int i = 0; i < entity_count; i++)
        {
            HeapTuple tuple = SPI_tuptable->vals[i];
            TupleDesc tupdesc = SPI_tuptable->tupdesc;
            bool isnull;

            char *entity_name = SPI_getvalue(tuple, tupdesc, 1);
            char *graphql_type = SPI_getvalue(tuple, tupdesc, 2);
            char *pk_column = SPI_getvalue(tuple, tupdesc, 3);
            char *data_table = SPI_getvalue(tuple, tupdesc, 4);
            char *source_table = SPI_getvalue(tuple, tupdesc, 5);
            char *delete_col = SPI_getvalue(tuple, tupdesc, 6);

            if (i > 0)
                appendStringInfoString(&view_sql, " UNION ALL ");

            appendStringInfo(&view_sql,
                "SELECT %s as id, '%s' as __typename, '%s' as entity_name, "
                "'%s' as source_table, data, created_at, updated_at "
                "FROM %s WHERE %s IS NULL",
                pk_column, graphql_type, entity_name, source_table,
                data_table, delete_col);
        }
    }

    /* Execute the view creation */
    if (SPI_execute(view_sql.data, false, 0) != SPI_OK_UTILITY)
        elog(ERROR, "failed to refresh v_nodes view");

    /* Try to create indexes (may fail for views, that's OK) */
    SPI_execute("DROP INDEX IF EXISTS core.idx_v_nodes_id", false, 0);
    SPI_execute("DROP INDEX IF EXISTS core.idx_v_nodes_typename", false, 0);
    SPI_execute("DROP INDEX IF EXISTS core.idx_v_nodes_entity_name", false, 0);

    if (entity_count > 0)
    {
        SPI_execute("CREATE INDEX idx_v_nodes_id ON core.v_nodes(id)", false, 0);
        SPI_execute("CREATE INDEX idx_v_nodes_typename ON core.v_nodes(__typename)", false, 0);
        SPI_execute("CREATE INDEX idx_v_nodes_entity_name ON core.v_nodes(entity_name)", false, 0);
    }

    SPI_finish();

    elog(NOTICE, "v_nodes view refreshed with %d entities", entity_count);

    PG_RETURN_BOOL(true);
}
