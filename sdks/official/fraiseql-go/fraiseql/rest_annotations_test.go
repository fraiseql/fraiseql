package fraiseql

import (
	"encoding/json"
	"testing"
)

func TestQueryRestAnnotation(t *testing.T) {
	Reset()
	defer Reset()
	err := NewQuery("users").
		ReturnType("User").
		ReturnsArray(true).
		SqlSource("v_user").
		RestPath("/api/users").
		RestMethod("GET").
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	schema := GetSchema()
	data, _ := json.Marshal(schema)
	var result map[string]interface{}
	json.Unmarshal(data, &result)

	queries := result["queries"].([]interface{})
	query := queries[0].(map[string]interface{})
	rest := query["rest"].(map[string]interface{})
	if rest["path"] != "/api/users" {
		t.Errorf("expected path /api/users, got %v", rest["path"])
	}
	if rest["method"] != "GET" {
		t.Errorf("expected method GET, got %v", rest["method"])
	}
}

func TestQueryRestDefaultMethodGET(t *testing.T) {
	Reset()
	defer Reset()
	err := NewQuery("users").
		ReturnType("User").
		ReturnsArray(true).
		SqlSource("v_user").
		RestPath("/api/users").
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	schema := GetSchema()
	data, _ := json.Marshal(schema)
	var result map[string]interface{}
	json.Unmarshal(data, &result)

	queries := result["queries"].([]interface{})
	query := queries[0].(map[string]interface{})
	rest := query["rest"].(map[string]interface{})
	if rest["method"] != "GET" {
		t.Errorf("expected default method GET, got %v", rest["method"])
	}
}

func TestQueryWithoutRestOmitsBlock(t *testing.T) {
	Reset()
	defer Reset()
	err := NewQuery("users").
		ReturnType("User").
		ReturnsArray(true).
		SqlSource("v_user").
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	schema := GetSchema()
	data, _ := json.Marshal(schema)
	var result map[string]interface{}
	json.Unmarshal(data, &result)

	queries := result["queries"].([]interface{})
	query := queries[0].(map[string]interface{})
	if _, ok := query["rest"]; ok {
		t.Error("expected no rest block when restPath not set")
	}
}

func TestMutationRestAnnotation(t *testing.T) {
	Reset()
	defer Reset()
	err := NewMutation("createUser").
		ReturnType("User").
		SqlSource("fn_create_user").
		Operation("insert").
		RestPath("/api/users").
		RestMethod("POST").
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	schema := GetSchema()
	data, _ := json.Marshal(schema)
	var result map[string]interface{}
	json.Unmarshal(data, &result)

	mutations := result["mutations"].([]interface{})
	mutation := mutations[0].(map[string]interface{})
	rest := mutation["rest"].(map[string]interface{})
	if rest["path"] != "/api/users" {
		t.Errorf("expected path /api/users, got %v", rest["path"])
	}
	if rest["method"] != "POST" {
		t.Errorf("expected method POST, got %v", rest["method"])
	}
}

func TestMutationRestDefaultMethodPOST(t *testing.T) {
	Reset()
	defer Reset()
	err := NewMutation("createUser").
		ReturnType("User").
		SqlSource("fn_create_user").
		Operation("insert").
		RestPath("/api/users").
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	schema := GetSchema()
	data, _ := json.Marshal(schema)
	var result map[string]interface{}
	json.Unmarshal(data, &result)

	mutations := result["mutations"].([]interface{})
	mutation := mutations[0].(map[string]interface{})
	rest := mutation["rest"].(map[string]interface{})
	if rest["method"] != "POST" {
		t.Errorf("expected default method POST, got %v", rest["method"])
	}
}

func TestMutationRestDeleteMethod(t *testing.T) {
	Reset()
	defer Reset()
	err := NewMutation("deleteUser").
		ReturnType("User").
		SqlSource("fn_delete_user").
		Operation("delete").
		RestPath("/api/users/{id}").
		RestMethod("DELETE").
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	schema := GetSchema()
	data, _ := json.Marshal(schema)
	var result map[string]interface{}
	json.Unmarshal(data, &result)

	mutations := result["mutations"].([]interface{})
	mutation := mutations[0].(map[string]interface{})
	rest := mutation["rest"].(map[string]interface{})
	if rest["path"] != "/api/users/{id}" {
		t.Errorf("expected path /api/users/{id}, got %v", rest["path"])
	}
	if rest["method"] != "DELETE" {
		t.Errorf("expected method DELETE, got %v", rest["method"])
	}
}

func TestRestMethodCaseInsensitive(t *testing.T) {
	Reset()
	defer Reset()
	err := NewQuery("users").
		ReturnType("User").
		ReturnsArray(true).
		SqlSource("v_user").
		RestPath("/api/users").
		RestMethod("get").
		Register()
	if err != nil {
		t.Fatalf("Register failed: %v", err)
	}

	schema := GetSchema()
	data, _ := json.Marshal(schema)
	var result map[string]interface{}
	json.Unmarshal(data, &result)

	queries := result["queries"].([]interface{})
	query := queries[0].(map[string]interface{})
	rest := query["rest"].(map[string]interface{})
	if rest["method"] != "GET" {
		t.Errorf("expected uppercase method GET, got %v", rest["method"])
	}
}
