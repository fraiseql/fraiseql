import { describe, it, expect, beforeEach } from "vitest";
import { SchemaRegistry } from "../src/registry";
import { registerQuery, registerMutation } from "../src/decorators";

describe("REST annotations", () => {
  beforeEach(() => {
    SchemaRegistry.clear();
  });

  describe("queries", () => {
    it("should include rest block when restPath is set", () => {
      registerQuery("users", "User", true, false, [], "Get users", {
        sqlSource: "v_user",
        restPath: "/api/users",
        restMethod: "GET",
      });
      const schema = SchemaRegistry.getSchema();
      expect(schema.queries[0].rest).toEqual({ path: "/api/users", method: "GET" });
    });

    it("should default method to GET for queries", () => {
      registerQuery("users", "User", true, false, [], "Get users", {
        sqlSource: "v_user",
        restPath: "/api/users",
      });
      const schema = SchemaRegistry.getSchema();
      expect(schema.queries[0].rest).toEqual({ path: "/api/users", method: "GET" });
    });

    it("should omit rest block when restPath is not set", () => {
      registerQuery("users", "User", true, false, [], "Get users", {
        sqlSource: "v_user",
      });
      const schema = SchemaRegistry.getSchema();
      expect(schema.queries[0].rest).toBeUndefined();
    });

    it("should uppercase the method", () => {
      registerQuery("users", "User", true, false, [], "Get users", {
        sqlSource: "v_user",
        restPath: "/api/users",
        restMethod: "post",
      });
      const schema = SchemaRegistry.getSchema();
      expect(schema.queries[0].rest?.method).toBe("POST");
    });
  });

  describe("mutations", () => {
    it("should include rest block when restPath is set", () => {
      registerMutation("createUser", "User", false, false, [], "Create user", {
        sqlSource: "fn_create_user",
        operation: "CREATE",
        restPath: "/api/users",
        restMethod: "POST",
      });
      const schema = SchemaRegistry.getSchema();
      expect(schema.mutations[0].rest).toEqual({ path: "/api/users", method: "POST" });
    });

    it("should default method to POST for mutations", () => {
      registerMutation("createUser", "User", false, false, [], "Create user", {
        sqlSource: "fn_create_user",
        operation: "CREATE",
        restPath: "/api/users",
      });
      const schema = SchemaRegistry.getSchema();
      expect(schema.mutations[0].rest?.method).toBe("POST");
    });

    it("should support DELETE method", () => {
      registerMutation("deleteUser", "User", false, false, [], "Delete user", {
        sqlSource: "fn_delete_user",
        operation: "DELETE",
        restPath: "/api/users/{id}",
        restMethod: "DELETE",
      });
      const schema = SchemaRegistry.getSchema();
      expect(schema.mutations[0].rest).toEqual({ path: "/api/users/{id}", method: "DELETE" });
    });

    it("should omit rest block when restPath is not set", () => {
      registerMutation("createUser", "User", false, false, [], "Create user", {
        sqlSource: "fn_create_user",
        operation: "CREATE",
      });
      const schema = SchemaRegistry.getSchema();
      expect(schema.mutations[0].rest).toBeUndefined();
    });
  });

  describe("validation", () => {
    it("should throw when restMethod is set without restPath", () => {
      expect(() => {
        registerQuery("users", "User", true, false, [], "Get users", {
          sqlSource: "v_user",
          restMethod: "GET",
        });
      }).toThrow("restMethod requires restPath to be set");
    });

    it("should throw for invalid REST method", () => {
      expect(() => {
        registerQuery("users", "User", true, false, [], "Get users", {
          sqlSource: "v_user",
          restPath: "/api/users",
          restMethod: "INVALID",
        });
      }).toThrow("Invalid REST method");
    });
  });
});
