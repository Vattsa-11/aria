import test from "node:test";
import assert from "node:assert/strict";
import { allowedImage } from "../src/image.ts";

test("only https images from known cover-art hosts are proxied", () => {
  assert.ok(allowedImage("https://c.saavncdn.com/871/cover-500x500.jpg"));
  assert.ok(allowedImage("https://i.ytimg.com/vi/abc/hqdefault.jpg"));
  assert.ok(allowedImage("https://lh3.googleusercontent.com/x=w120"));
  assert.equal(allowedImage("http://c.saavncdn.com/a.jpg"), null);
  assert.equal(allowedImage("https://evil.com/saavncdn.com/a.jpg"), null);
  assert.equal(allowedImage("https://saavncdn.com.evil.com/a.jpg"), null);
  assert.equal(allowedImage("https://user:pw@c.saavncdn.com/a.jpg"), null);
  assert.equal(allowedImage("https://c.saavncdn.com:8443/a.jpg"), null);
  assert.equal(allowedImage("not a url"), null);
  assert.equal(allowedImage("https://localhost/a.jpg"), null);
});
