//! SQL pipeline scaffolding (parser, planner, executor interfaces).
+
+#![allow(dead_code)]
+
+/// Draft marker for the SQL engine blueprint.
+#[derive(Debug, Default, Clone)]
+pub struct SqlEngineBlueprint;
+
+impl SqlEngineBlueprint {
+    pub fn new() -> Self {
+        Self
+    }
+
+    pub fn describe(&self) -> &'static str {
+        "sql-engine-blueprint"
+    }
+}
+EOF
