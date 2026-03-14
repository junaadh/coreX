; Indent for block-like containers and grouped import trees.
[
  (block)
  (struct_body)
  (enum_body)
  (impl_body)
  (protocol_body)
  (extern_body)
  (use_group)
  (parameter_list)
  (argument_list)
  (array_literal)
  (array_pattern)
  (tuple_pattern)
  (attribute_arguments)
  (attribute_block)
  (match_expression)
] @indent

; Outdent on closing delimiters.
[
  "}"
  ")"
  "]"
] @outdent
