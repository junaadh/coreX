; Function-like containers
(function_declaration) @function.around
(function_declaration (block) @function.inside)

(initializer_declaration) @function.around
(initializer_declaration (block) @function.inside)

(protocol_function_requirement) @function.around
(protocol_function_requirement (block) @function.inside)

(protocol_initializer_requirement) @function.around
(protocol_initializer_requirement (block) @function.inside)

(extern_function_declaration) @function.around

; Class-like containers
(struct_declaration) @class.around
(struct_declaration (struct_body) @class.inside)

(enum_declaration) @class.around
(enum_declaration (enum_body) @class.inside)

(protocol_declaration) @class.around
(protocol_declaration (protocol_body) @class.inside)

(extern_block) @class.around
(extern_block (extern_body) @class.inside)

; Block-like containers
(block) @block.around
(block) @block.inside

(use_group) @block.around
(use_group) @block.inside

; Parameters and arguments
(parameter_list) @parameter.around
(parameter) @parameter.around
(labeled_parameter) @parameter.inside
(receiver_parameter) @parameter.inside

(argument_list) @parameter.around
(argument) @parameter.around
(argument
  value: (_) @parameter.inside)

; Comment objects
(comment) @comment.around
(comment) @comment.inside
(doc_comment) @comment.around
(doc_comment) @comment.inside
(inner_doc_comment) @comment.around
(inner_doc_comment) @comment.inside
