# Pure Function Benchmark Results

**Purpose:** Measure ONLY function overhead, no joins or subquery overhead

**Date:** ven. 17 oct. 2025 00:13:50 CEST
**Database:** postgres
**Test Duration:** 30 seconds per test
**Concurrency:** 10 clients, 4 jobs

## Test Setup

These tests use **direct function calls** with no LATERAL joins or nested subqueries:

- **jsonb_build_object**: `SELECT jsonb_build_object(...) FROM table`
- **row_to_json**: `SELECT row_to_json(ROW(...)) FROM table` (no subquery!)
- **to_jsonb**: `SELECT to_jsonb(table) FROM table`

This isolates pure function performance from query pattern overhead.

---

## Results


## Test 1: Full Table Scan (10,000 rows)

Pure function overhead for full table scans.

### jsonb_build_object (pure)

- **TPS:** 8.540795
- **Latency (avg):** 466.554 ms
- **Latency (stddev):** 209.759 ms

<details>
<summary>Full Output</summary>

```
pgbench (17.5)
starting vacuum...pgbench: error: ERROR:  relation "pgbench_branches" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_tellers" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_history" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
end.
progress: 1760652835.586 s, 8.8 tps, lat 428.850 ms stddev 104.032, 0 failed
progress: 1760652840.586 s, 9.8 tps, lat 393.514 ms stddev 98.006, 0 failed
progress: 1760652845.586 s, 9.4 tps, lat 449.828 ms stddev 181.911, 0 failed
progress: 1760652850.586 s, 8.0 tps, lat 458.931 ms stddev 230.325, 0 failed
progress: 1760652855.586 s, 7.2 tps, lat 581.689 ms stddev 345.926, 0 failed
progress: 1760652860.586 s, 7.8 tps, lat 517.876 ms stddev 191.071, 0 failed
transaction type: /tmp/pure_tests/full_scan_temp.sql
scaling factor: 1
query mode: simple
number of clients: 4
number of threads: 2
maximum number of tries: 1
duration: 30 s
number of transactions actually processed: 259
number of failed transactions: 0 (0.000%)
latency average = 466.554 ms
latency stddev = 209.759 ms
initial connection time = 17.975 ms
tps = 8.540795 (without initial connection time)
```

</details>

### row_to_json(ROW(...)) (pure)

- **TPS:** 6.864904
- **Latency (avg):** 580.894 ms
- **Latency (stddev):** 229.004 ms

<details>
<summary>Full Output</summary>

```
pgbench (17.5)
starting vacuum...pgbench: error: ERROR:  relation "pgbench_branches" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_tellers" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_history" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
end.
progress: 1760652865.982 s, 5.8 tps, lat 647.301 ms stddev 244.419, 0 failed
progress: 1760652870.980 s, 5.2 tps, lat 752.724 ms stddev 348.478, 0 failed
progress: 1760652875.980 s, 6.2 tps, lat 645.263 ms stddev 176.317, 0 failed
progress: 1760652880.980 s, 7.8 tps, lat 507.932 ms stddev 167.981, 0 failed
progress: 1760652885.980 s, 9.0 tps, lat 468.517 ms stddev 146.728, 0 failed
progress: 1760652890.980 s, 6.8 tps, lat 570.921 ms stddev 179.329, 0 failed
transaction type: /tmp/pure_tests/full_scan_temp.sql
scaling factor: 1
query mode: simple
number of clients: 4
number of threads: 2
maximum number of tries: 1
duration: 30 s
number of transactions actually processed: 208
number of failed transactions: 0 (0.000%)
latency average = 580.894 ms
latency stddev = 229.004 ms
initial connection time = 14.962 ms
tps = 6.864904 (without initial connection time)
```

</details>

### to_jsonb (pure)

- **TPS:** 7.330543
- **Latency (avg):** 544.788 ms
- **Latency (stddev):** 195.181 ms

<details>
<summary>Full Output</summary>

```
pgbench (17.5)
starting vacuum...pgbench: error: ERROR:  relation "pgbench_branches" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_tellers" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_history" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
end.
progress: 1760652896.349 s, 7.2 tps, lat 526.574 ms stddev 154.722, 0 failed
progress: 1760652901.349 s, 6.8 tps, lat 583.807 ms stddev 217.776, 0 failed
progress: 1760652906.349 s, 7.2 tps, lat 551.951 ms stddev 155.456, 0 failed
progress: 1760652911.349 s, 7.2 tps, lat 519.359 ms stddev 207.935, 0 failed
progress: 1760652916.349 s, 7.6 tps, lat 562.993 ms stddev 242.446, 0 failed
progress: 1760652921.349 s, 7.8 tps, lat 524.315 ms stddev 175.203, 0 failed
transaction type: /tmp/pure_tests/full_scan_temp.sql
scaling factor: 1
query mode: simple
number of clients: 4
number of threads: 2
maximum number of tries: 1
duration: 30 s
number of transactions actually processed: 223
number of failed transactions: 0 (0.000%)
latency average = 544.788 ms
latency stddev = 195.181 ms
initial connection time = 14.555 ms
tps = 7.330543 (without initial connection time)
```

</details>


## Test 2: Paginated Query (100 rows)

Pure function overhead for paginated queries.

### jsonb_build_object (pure)

- **TPS:** 17.235708
- **Latency (avg):** 576.496 ms
- **Latency (stddev):** 453.290 ms

<details>
<summary>Full Output</summary>

```
pgbench (17.5)
starting vacuum...pgbench: error: ERROR:  relation "pgbench_branches" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_tellers" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_history" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
end.
progress: 1760652926.818 s, 17.8 tps, lat 508.854 ms stddev 378.494, 0 failed
progress: 1760652931.818 s, 20.6 tps, lat 506.832 ms stddev 415.045, 0 failed
progress: 1760652936.819 s, 17.4 tps, lat 537.700 ms stddev 451.293, 0 failed
progress: 1760652941.818 s, 16.8 tps, lat 585.886 ms stddev 407.095, 0 failed
progress: 1760652946.818 s, 14.0 tps, lat 728.201 ms stddev 568.450, 0 failed
progress: 1760652951.818 s, 16.4 tps, lat 602.198 ms stddev 472.182, 0 failed
transaction type: /tmp/pure_tests/paginated_temp.sql
scaling factor: 1
query mode: simple
number of clients: 10
number of threads: 4
maximum number of tries: 1
duration: 30 s
number of transactions actually processed: 525
number of failed transactions: 0 (0.000%)
latency average = 576.496 ms
latency stddev = 453.290 ms
initial connection time = 24.477 ms
tps = 17.235708 (without initial connection time)
```

</details>

### row_to_json(ROW(...)) (pure)

- **TPS:** 13.305055
- **Latency (avg):** 748.103 ms
- **Latency (stddev):** 618.074 ms

<details>
<summary>Full Output</summary>

```
pgbench (17.5)
starting vacuum...pgbench: error: ERROR:  relation "pgbench_branches" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_tellers" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_history" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
end.
progress: 1760652957.361 s, 15.8 tps, lat 588.140 ms stddev 454.901, 0 failed
progress: 1760652962.358 s, 12.8 tps, lat 689.138 ms stddev 550.256, 0 failed
progress: 1760652967.360 s, 9.2 tps, lat 1030.970 ms stddev 645.531, 0 failed
progress: 1760652972.358 s, 13.6 tps, lat 847.068 ms stddev 780.443, 0 failed
progress: 1760652977.362 s, 12.4 tps, lat 816.935 ms stddev 633.181, 0 failed
progress: 1760652982.358 s, 15.4 tps, lat 575.587 ms stddev 500.719, 0 failed
transaction type: /tmp/pure_tests/paginated_temp.sql
scaling factor: 1
query mode: simple
number of clients: 10
number of threads: 4
maximum number of tries: 1
duration: 30 s
number of transactions actually processed: 406
number of failed transactions: 0 (0.000%)
latency average = 748.103 ms
latency stddev = 618.074 ms
initial connection time = 29.246 ms
tps = 13.305055 (without initial connection time)
```

</details>

### to_jsonb (pure)

- **TPS:** 16.276610
- **Latency (avg):** 609.320 ms
- **Latency (stddev):** 485.617 ms

<details>
<summary>Full Output</summary>

```
pgbench (17.5)
starting vacuum...pgbench: error: ERROR:  relation "pgbench_branches" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_tellers" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_history" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
end.
progress: 1760652987.990 s, 12.0 tps, lat 734.997 ms stddev 617.739, 0 failed
progress: 1760652992.992 s, 17.6 tps, lat 588.831 ms stddev 464.269, 0 failed
progress: 1760652997.990 s, 17.4 tps, lat 583.433 ms stddev 421.142, 0 failed
progress: 1760653002.990 s, 15.8 tps, lat 605.758 ms stddev 480.645, 0 failed
progress: 1760653007.995 s, 18.4 tps, lat 555.466 ms stddev 466.499, 0 failed
progress: 1760653012.990 s, 15.8 tps, lat 623.531 ms stddev 476.334, 0 failed
transaction type: /tmp/pure_tests/paginated_temp.sql
scaling factor: 1
query mode: simple
number of clients: 10
number of threads: 4
maximum number of tries: 1
duration: 30 s
number of transactions actually processed: 495
number of failed transactions: 0 (0.000%)
latency average = 609.320 ms
latency stddev = 485.617 ms
initial connection time = 44.493 ms
tps = 16.276610 (without initial connection time)
```

</details>


## Test 3: Filtered Query

Pure function overhead for filtered queries.

### jsonb_build_object (pure)

- **TPS:** 541.588685
- **Latency (avg):** 17.774 ms
- **Latency (stddev):** 30.651 ms

<details>
<summary>Full Output</summary>

```
pgbench (17.5)
starting vacuum...pgbench: error: ERROR:  relation "pgbench_branches" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_tellers" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_history" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
end.
progress: 1760653018.505 s, 499.8 tps, lat 19.219 ms stddev 30.142, 0 failed
progress: 1760653023.505 s, 517.8 tps, lat 18.716 ms stddev 34.539, 0 failed
progress: 1760653028.506 s, 605.5 tps, lat 15.385 ms stddev 23.463, 0 failed
progress: 1760653033.505 s, 510.1 tps, lat 19.041 ms stddev 32.476, 0 failed
progress: 1760653038.505 s, 584.2 tps, lat 16.473 ms stddev 32.145, 0 failed
progress: 1760653043.515 s, 537.4 tps, lat 18.036 ms stddev 29.562, 0 failed
transaction type: /tmp/pure_tests/filtered_temp.sql
scaling factor: 1
query mode: simple
number of clients: 10
number of threads: 4
maximum number of tries: 1
duration: 30 s
number of transactions actually processed: 16289
number of failed transactions: 0 (0.000%)
latency average = 17.774 ms
latency stddev = 30.651 ms
initial connection time = 37.525 ms
tps = 541.588685 (without initial connection time)
```

</details>

### row_to_json(ROW(...)) (pure)

- **TPS:** 502.035004
- **Latency (avg):** 19.405 ms
- **Latency (stddev):** 32.483 ms

<details>
<summary>Full Output</summary>

```
pgbench (17.5)
starting vacuum...pgbench: error: ERROR:  relation "pgbench_branches" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_tellers" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_history" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
end.
progress: 1760653048.704 s, 463.8 tps, lat 20.947 ms stddev 36.459, 0 failed
progress: 1760653053.770 s, 512.4 tps, lat 18.834 ms stddev 30.810, 0 failed
progress: 1760653058.704 s, 519.9 tps, lat 18.201 ms stddev 26.199, 0 failed
progress: 1760653063.704 s, 536.0 tps, lat 18.611 ms stddev 28.828, 0 failed
progress: 1760653068.704 s, 444.2 tps, lat 22.176 ms stddev 38.886, 0 failed
progress: 1760653073.704 s, 538.4 tps, lat 17.920 ms stddev 32.185, 0 failed
transaction type: /tmp/pure_tests/filtered_temp.sql
scaling factor: 1
query mode: simple
number of clients: 10
number of threads: 4
maximum number of tries: 1
duration: 30 s
number of transactions actually processed: 15081
number of failed transactions: 0 (0.000%)
latency average = 19.405 ms
latency stddev = 32.483 ms
initial connection time = 56.044 ms
tps = 502.035004 (without initial connection time)
```

</details>

### to_jsonb (pure)

- **TPS:** 516.793677
- **Latency (avg):** 18.539 ms
- **Latency (stddev):** 32.393 ms

<details>
<summary>Full Output</summary>

```
pgbench (17.5)
starting vacuum...pgbench: error: ERROR:  relation "pgbench_branches" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_tellers" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
pgbench: error: ERROR:  relation "pgbench_history" does not exist
pgbench: detail: (ignoring this error and continuing anyway)
end.
progress: 1760653078.859 s, 470.7 tps, lat 20.077 ms stddev 27.351, 0 failed
progress: 1760653083.864 s, 441.7 tps, lat 22.056 ms stddev 38.086, 0 failed
progress: 1760653088.870 s, 582.7 tps, lat 16.084 ms stddev 25.288, 0 failed
progress: 1760653093.863 s, 518.1 tps, lat 18.533 ms stddev 41.379, 0 failed
progress: 1760653098.859 s, 509.5 tps, lat 19.148 ms stddev 29.642, 0 failed
progress: 1760653103.864 s, 575.9 tps, lat 16.404 ms stddev 29.876, 0 failed
transaction type: /tmp/pure_tests/filtered_temp.sql
scaling factor: 1
query mode: simple
number of clients: 10
number of threads: 4
maximum number of tries: 1
duration: 30 s
number of transactions actually processed: 15501
number of failed transactions: 0 (0.000%)
latency average = 18.539 ms
latency stddev = 32.393 ms
initial connection time = 43.179 ms
tps = 516.793677 (without initial connection time)
```

</details>


---

## Summary

### Full Table Scan Performance

| Method | TPS | vs jsonb_build_object |
|--------|-----|-----------------------|
| jsonb_build_object | 8.540795 | Baseline |
| row_to_json(ROW(...)) | 6.864904 | -19.6% |
| to_jsonb | 516.793677 | 5950.9% |

---

## Key Findings

1. **Pure Function Performance**: These results show the actual function overhead WITHOUT query pattern complications
2. **Comparison to Full Benchmark**: Differences between this and the full benchmark reveal query pattern overhead (LATERAL joins, subqueries)
3. **PostgreSQL Version**: Functions may have different performance characteristics across PostgreSQL versions

---

**Benchmark completed:** ven. 17 oct. 2025 00:18:24 CEST
