(comment) @comment
(doc_comment) @comment.documentation
(inner_doc_comment) @comment.documentation

(attribute
  "@" @punctuation.special
  name: (identifier) @attribute)

(macro_expression
  "@" @punctuation.special
  name: (identifier) @function.macro)

(function_declaration
  name: (identifier) @function)

(protocol_function_requirement
  name: (identifier) @function)

(extern_function_declaration
  name: (identifier) @function)

(initializer_declaration
  "init" @constructor)

(protocol_initializer_requirement
  "init" @constructor)

(struct_declaration
  name: (identifier) @type)

(enum_declaration
  name: (identifier) @type)

(protocol_declaration
  name: (identifier) @type)

(associated_type_declaration
  name: (identifier) @type)

(named_type
  (identifier) @type)

(self_type) @type.builtin
((named_type
   (identifier) @type.builtin)
  (#match? @type.builtin "^(u8|u16|u32|u64|usize|i8|i16|i32|i64|isize|f32|f64|bool|char|string|void|Self|Option|Result)$"))

(struct_field
  name: (identifier) @property)

(protocol_property_requirement
  name: (identifier) @property)

(struct_pattern_field
  name: (identifier) @property)

(struct_literal_field
  name: (identifier) @property)

(shorthand_member_expression
  member: (identifier) @property)

(postfix_suffix
  member: (identifier) @property)

(enum_variant
  name: (identifier) @constructor)

(variant_pattern
  name: (identifier) @constructor)

(qualified_enum_case_expression
  type: (identifier) @type
  case: (identifier) @constructor)

(parameter
  (labeled_parameter
    name: (identifier) @parameter))

(closure_parameter
  name: (identifier) @parameter)

(identifier_pattern
  (identifier) @variable)

(receiver_parameter
  "self" @variable.builtin)

(self_expression) @variable.builtin

(extern_block
  library: (identifier) @namespace)

(scope_declaration
  name: (identifier) @namespace)

(use_path_segment
  (identifier) @namespace)

(use_name) @namespace

(use_alias
  alias: (identifier) @namespace)

(visibility
  "pub" @keyword.modifier)

(visibility
  [
    "super"
    "project"
  ] @keyword.modifier)

(use_alias
  "as" @keyword.operator)

(self_import) @variable.builtin

(use_path_segment
  [
    "root"
    "self"
    "super"
  ] @keyword)

(modifier) @keyword.modifier

[
  "fn"
  "scope"
  "struct"
  "enum"
  "impl"
  "protocol"
  "extern"
  "type"
  "use"
  "where"
] @keyword

[
  "if"
  "else"
  "match"
  "guard"
  "while"
  "for"
  "in"
  "return"
  "break"
  "continue"
] @keyword.control

[
  "let"
  "var"
] @keyword

[
  "try"
  "as"
  "as?"
] @keyword.operator

(assignment_operator) @operator
(prefix_operator) @operator
(cast_operator) @operator

[
  "??"
  "?"
  ":"
  ".."
  "..="
  "."
  "::"
  "->"
  "=>"
  "+"
  "-"
  "*"
  "/"
  "%"
  "=="
  "!="
  "<"
  "<="
  ">"
  ">="
  "&&"
  "||"
  "&"
  "|"
  "^"
  "<<"
  ">>"
  "="
  "+="
  "-="
  "*="
  "/="
  "%="
  "&="
  "|="
  "^="
  "<<="
  ">>="
  "!"
] @operator

(integer_literal) @number
(float_literal) @number.float
(boolean_literal) @boolean
(char_literal) @string.special
(string_literal) @string

[
  "("
  ")"
  "["
  "]"
  "{"
  "}"
] @punctuation.bracket

[
  ","
  ";"
] @punctuation.delimiter
