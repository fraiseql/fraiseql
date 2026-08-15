package fraiseql

import (
	"encoding/json"
	"strings"
	"testing"
)

// pgvector field authoring: vector_config and vector_distance (#959).
//
// The compiler refuses a Vector field carrying no vector_config, so an SDK that
// cannot author the config cannot author the type at all. These tests follow the
// declaration all the way to the exported JSON, because the defect this surface
// exists to prevent is one that survives authoring and disappears before the
// compiler sees it — which is what #807 was, on this SDK, for `requires_scope`.

func documentFields(t *testing.T) map[string]map[string]any {
	t.Helper()
	data, err := json.Marshal(GetSchema())
	if err != nil {
		t.Fatalf("marshal schema: %v", err)
	}
	var schema struct {
		Types []struct {
			Name   string           `json:"name"`
			Fields []map[string]any `json:"fields"`
		} `json:"types"`
	}
	if err := json.Unmarshal(data, &schema); err != nil {
		t.Fatalf("unmarshal schema: %v", err)
	}
	for _, typ := range schema.Types {
		if typ.Name != "Document" {
			continue
		}
		fields := map[string]map[string]any{}
		for _, f := range typ.Fields {
			fields[f["name"].(string)] = f
		}
		return fields
	}
	t.Fatal("type Document is absent from the exported schema")
	return nil
}

func TestEveryVectorFieldTypeCarriesItsConfig(t *testing.T) {
	Reset()
	defer Reset()

	err := RegisterType("Document", []FieldInfo{
		{Name: "id", Type: "ID"},
		{Name: "embedding", Type: "Vector",
			Vector: NewVectorConfig(1536).WithIndex(IndexIVFFlat).WithMetric(MetricL2)},
		{Name: "fingerprint", Type: "BitVector",
			Vector: NewVectorConfig(768).WithMetric(MetricHamming)},
		{Name: "compact", Type: "HalfVector", Nullable: true,
			Vector: NewVectorConfig(1536).WithMetric(MetricInnerProduct)},
		{Name: "terms", Type: "SparseVector", Nullable: true,
			Vector: NewVectorConfig(30000).WithIndex(IndexNone)},
	}, "")
	if err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	fields := documentFields(t)
	// Every key is asserted, not just the object's presence: index_type and
	// distance_metric both have compiler-side defaults, so a config that lost them
	// would still compile — to hnsw + cosine, chosen by nobody.
	embedding, ok := fields["embedding"]["vector_config"].(map[string]any)
	if !ok {
		t.Fatalf("embedding carries no vector_config: %v", fields["embedding"])
	}
	if embedding["dimensions"] != float64(1536) ||
		embedding["index_type"] != "ivf_flat" ||
		embedding["distance_metric"] != "l2" {
		t.Errorf("embedding config: %v", embedding)
	}
	for name, metric := range map[string]string{
		"fingerprint": "hamming",
		"compact":     "inner_product",
	} {
		got := fields[name]["vector_config"].(map[string]any)["distance_metric"]
		if got != metric {
			t.Errorf("%s metric: got %v, want %v", name, got, metric)
		}
	}
	if got := fields["terms"]["vector_config"].(map[string]any)["index_type"]; got != "none" {
		t.Errorf("terms index: got %v, want none", got)
	}
}

func TestDistanceFieldNamesTheVectorItMeasures(t *testing.T) {
	Reset()
	defer Reset()

	err := RegisterType("Document", []FieldInfo{
		{Name: "embedding", Type: "Vector", Vector: NewVectorConfig(8)},
		{Name: "similarity", Type: "Float", VectorDistance: "embedding"},
	}, "")
	if err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	if got := documentFields(t)["similarity"]["vector_distance"]; got != "embedding" {
		t.Errorf("vector_distance: got %v, want embedding", got)
	}
}

func TestAFieldIsAnEmbeddingOrADistanceNotBoth(t *testing.T) {
	Reset()
	defer Reset()

	err := RegisterType("Document", []FieldInfo{
		{Name: "embedding", Type: "Vector", Vector: NewVectorConfig(8), VectorDistance: "embedding"},
	}, "")
	if err == nil || !strings.Contains(err.Error(), "not both") {
		t.Errorf("expected a refusal naming the conflict, got %v", err)
	}
}

func TestADimensionCountNoColumnCanHaveIsRefused(t *testing.T) {
	Reset()
	defer Reset()

	err := RegisterType("Document", []FieldInfo{
		{Name: "embedding", Type: "Vector", Vector: &VectorConfig{Dimensions: 0}},
	}, "")
	if err == nil || !strings.Contains(err.Error(), "at least 1") {
		t.Errorf("expected a refusal naming the dimension floor, got %v", err)
	}
}

func TestANonVectorFieldEmitsNoVectorKeys(t *testing.T) {
	Reset()
	defer Reset()

	if err := RegisterType("Document", []FieldInfo{{Name: "id", Type: "ID"}}, ""); err != nil {
		t.Fatalf("RegisterType: %v", err)
	}

	id := documentFields(t)["id"]
	if _, present := id["vector_config"]; present {
		t.Errorf("an ordinary field must not carry a vector_config: %v", id)
	}
	if _, present := id["vector_distance"]; present {
		t.Errorf("an ordinary field must not carry a vector_distance: %v", id)
	}
}
