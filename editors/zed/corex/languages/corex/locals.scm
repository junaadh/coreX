(source_file) @local.scope
(block) @local.scope
(closure_expression) @local.scope
(match_arm) @local.scope

(parameter
  (labeled_parameter
    name: (identifier) @local.definition))

(closure_parameter
  name: (identifier) @local.definition)

(identifier_pattern
  (identifier) @local.definition)

(primary_expression
  (identifier) @local.reference)
