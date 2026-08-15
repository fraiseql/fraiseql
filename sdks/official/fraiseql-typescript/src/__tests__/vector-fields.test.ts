import { SchemaRegistry } from "../registry";
import { registerTypeFields } from "../decorators";

/**
 * pgvector field authoring: `vectorConfig` and `vectorDistance` (#959).
 *
 * The compiler refuses a `Vector` field carrying no `vector_config`, so an SDK that
 * cannot author the config cannot author the type at all. The keys are documented
 * camelCase and read snake_case, which is exactly the shape of #925: a metadata key
 * that reached `fraiseql compile` in this SDK's own spelling and was refused by name.
 */
describe("vector field authoring", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  const fieldOf = (typeName: string, fieldName: string): Record<string, unknown> => {
    const type = SchemaRegistry.getSchema().types.find((t) => t.name === typeName)!;
    return type.fields.find((f) => f.name === fieldName)! as unknown as Record<string, unknown>;
  };

  it("emits every vector field type with the keys the compiler reads", () => {
    registerTypeFields(
      "Document",
      [
        { name: "id", type: "ID", nullable: false },
        {
          name: "embedding",
          type: "Vector",
          nullable: false,
          vectorConfig: { dimensions: 1536, indexType: "ivf_flat", distanceMetric: "l2" },
        },
        {
          name: "fingerprint",
          type: "BitVector",
          nullable: false,
          vectorConfig: { dimensions: 768, distanceMetric: "hamming" },
        },
        {
          name: "compact",
          type: "HalfVector",
          nullable: true,
          vectorConfig: { dimensions: 1536, distanceMetric: "inner_product" },
        },
        {
          name: "terms",
          type: "SparseVector",
          nullable: true,
          vectorConfig: { dimensions: 30000, indexType: "none" },
        },
      ],
      undefined,
      { sqlSource: "v_document" }
    );

    expect(fieldOf("Document", "embedding").vector_config).toEqual({
      dimensions: 1536,
      index_type: "ivf_flat",
      distance_metric: "l2",
    });
    expect(fieldOf("Document", "embedding").vectorConfig).toBeUndefined();
    expect(fieldOf("Document", "fingerprint").vector_config).toEqual({
      dimensions: 768,
      index_type: "hnsw",
      distance_metric: "hamming",
    });
    expect(fieldOf("Document", "compact").vector_config).toMatchObject({ distance_metric: "inner_product" });
    expect(fieldOf("Document", "terms").vector_config).toMatchObject({ index_type: "none" });
  });

  it("writes out the index and metric the author left to the default", () => {
    // A `vector_config` that carries only `dimensions` compiles — to hnsw + cosine,
    // chosen by nobody. The emitted schema says which index and which metric the
    // column will get, so the choice is visible where the author can see it.
    registerTypeFields(
      "Document",
      [
        { name: "id", type: "ID", nullable: false },
        {
          name: "embedding",
          type: "Vector",
          nullable: false,
          vectorConfig: { dimensions: 8 },
        },
      ],
      undefined,
      { sqlSource: "v_document" }
    );

    expect(fieldOf("Document", "embedding").vector_config).toEqual({
      dimensions: 8,
      index_type: "hnsw",
      distance_metric: "cosine",
    });
  });

  it("carries a distance field's reference to the vector it measures", () => {
    registerTypeFields(
      "Document",
      [
        {
          name: "embedding",
          type: "Vector",
          nullable: false,
          vectorConfig: { dimensions: 8 },
        },
        { name: "similarity", type: "Float", nullable: false, vectorDistance: "embedding" },
      ],
      undefined,
      { sqlSource: "v_document" }
    );

    expect(fieldOf("Document", "similarity").vector_distance).toBe("embedding");
    expect(fieldOf("Document", "similarity").vectorDistance).toBeUndefined();
  });

  it("refuses a field that is both an embedding and a distance", () => {
    expect(() =>
      registerTypeFields(
        "Document",
        [
          {
            name: "embedding",
            type: "Vector",
            nullable: false,
            vectorConfig: { dimensions: 8 },
            vectorDistance: "embedding",
          },
        ],
        undefined,
        { sqlSource: "v_document" }
      )
    ).toThrow(/both vectorConfig and vectorDistance/);
  });

  it("refuses a dimension count no column can have", () => {
    expect(() =>
      registerTypeFields(
        "Document",
        [
          {
            name: "embedding",
            type: "Vector",
            nullable: false,
            vectorConfig: { dimensions: 0 },
          },
        ],
        undefined,
        { sqlSource: "v_document" }
      )
    ).toThrow(/at least 1/);
  });
});
