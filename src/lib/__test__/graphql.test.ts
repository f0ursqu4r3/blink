import { describe, expect, it } from "vitest";
import { formatGraphql } from "../graphql";

describe("GraphQL formatting", () => {
  it("formats queries and keeps comments", async () => {
    expect(
      await formatGraphql(
        "# fetch\nquery Q($id:ID!){node(id:$id){id ...on User{name}}}",
      ),
    ).toBe(
      [
        "# fetch",
        "query Q($id: ID!) {",
        "  node(id: $id) {",
        "    id",
        "    ... on User {",
        "      name",
        "    }",
        "  }",
        "}",
      ].join("\n"),
    );
  });
  it("rejects invalid GraphQL", async () => {
    await expect(formatGraphql("query {")).rejects.toThrow();
  });
});
