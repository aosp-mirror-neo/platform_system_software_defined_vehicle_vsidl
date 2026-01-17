/*
 * Copyright (C) 2026 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#[derive(pest_derive::Parser)]
#[grammar_inline = r##"
WHITESPACE = _{ " " | "\t" | "\r" | "\n" }
COMMENT = { single_line_comment | multi_line_comment }
single_line_comment = { "//" ~ (!NEWLINE ~ ANY)* }
multi_line_comment = { "/*" ~ (!"*/" ~ ANY)* ~ "*/" }

// ------------------------------------
// Core Rules
// ------------------------------------

proto = { SOI ~ syntax? ~ (import | edition | package | option | topLevelDef | emptyStatement)* ~ EOI }
topLevelDef = { message | enum_ | extend | service }

syntax = { "syntax" ~ "=" ~ strLit ~ ";" }
edition = { "edition" ~ "=" ~ strLit }
import = { "import" ~ (weak | public | "option")? ~ strLit ~ ";" }
package = { "package" ~ fullIdent ~ ";" }
option = { "option" ~ optionName ~ "=" ~ constant ~ ";" }
optionName = { (ident | bracedFullIdent) ~ ("." ~ (ident | bracedFullIdent))* }
bracedFullIdent = { "(" ~ "."? ~ fullIdent ~ ")" }
public = { "public" }
weak = { "weak" }

// ------------------------------------
// Top-Level Definitions
// ------------------------------------

enum_ = { "enum" ~ enumName ~ enumBody }
enumBody = { "{" ~ (option | enumField | emptyStatement | reserved)* ~ "}" }
enumField = { ident ~ "=" ~ ("-")? ~ intLit ~ ("[" ~ enumValueOption ~ ("," ~ enumValueOption)* ~ "]")? ~ ";" }
enumValueOption = { optionName ~ "=" ~ constant }

message = { "message" ~ messageName ~ messageBody }
messageBody = { "{" ~ (enum_ | message | extend | extensions | group | option | oneof | mapField | reserved | field | emptyStatement)* ~ "}" }

service = { "service" ~ serviceName ~ "{" ~ (option | rpc | emptyStatement)* ~ "}" }
rpc = { "rpc" ~ rpcName ~ "(" ~ client_stream? ~ messageType ~ ")" ~ "returns" ~ "(" ~ server_stream? ~ messageType ~ ")" ~ (("{" ~ (option | emptyStatement)* ~ "}") | ";") }
client_stream = { "stream" }
server_stream = { "stream" }


// ------------------------------------
// Message Field Rules
// ------------------------------------

label = { "required" | "optional" | "repeated" }
type_ = { "double" | "float" | "int32" | "int64" | "uint32" | "uint64" | "sint32" | "sint64" | "fixed32" | "fixed64" | "sfixed32" | "sfixed64" | "bool" | "string" | "bytes" | messageType | enumType }
fieldNumber = { intLit }
fieldOptions = { fieldOption ~ ("," ~ fieldOption)* }
fieldOption = { optionName ~ "=" ~ constant }
fieldAssignment = { ident ~ ":" ~ (arrayConstant | constant) }
messageType = { ("."? ~ (ident ~ ".")*) ~ messageName }
enumType = { ("."? ~ (ident ~ ".")*) ~ enumName }

// Unifies field definitions for Proto2 and Proto3
field = { label? ~ type_ ~ fieldName ~ "=" ~ fieldNumber ~ ("[" ~ fieldOptions ~ "]")? ~ ";" }

// Proto2-specific rules
group = { label ~ "group" ~ groupName ~ "=" ~ fieldNumber ~ messageBody }
extensions = { "extensions" ~ ranges ~ ("[" ~ fieldOptions ~ "]")? ~ ";" }
extend = { "extend" ~ messageType ~ "{" ~ (group | field)* ~ "}" }

oneof = { "oneof" ~ oneofName ~ "{" ~ (option | oneofField)* ~ "}" }
oneofField = { type_ ~ fieldName ~ "=" ~ fieldNumber ~ ("[" ~ fieldOptions ~ "]")? ~ ";" }

mapField = { "map" ~ "<" ~ keyType ~ "," ~ type_ ~ ">" ~ mapName ~ "=" ~ fieldNumber ~ ("[" ~ fieldOptions ~ "]")? ~ ";" }
keyType = { "int32" | "int64" | "uint32" | "uint64" | "sint32" | "sint64" | "fixed32" | "fixed64" | "sfixed32" | "sfixed64" | "bool" | "string" }

reserved = { "reserved" ~ (ranges | strFieldNames | idents ) ~ ";" }
ranges = { range_ ~ ("," ~ range_)* }
range_ = { intLit ~ ("to" ~ (intLit | "max"))? }
strFieldNames = { strFieldName ~ ("," ~ strFieldName)* }
strFieldName = { "'" ~ fieldName ~ "'" | "\"" ~ fieldName ~ "\"" }
idents = { ident ~ ("," ~ ident)* }

// ------------------------------------
// Lexical Elements
// ------------------------------------

letter = @{ 'A'..'Z' | 'a'..'z' }
capitalLetter = @{ 'A'..'Z' }
decimalDigit = @{ '0'..'9' }
octalDigit = @{ '0'..'7' }
hexDigit = @{ '0'..'9' | 'A'..'F' | 'a'..'f' }

ident = @{ (ASCII_ALPHA | "_") ~ (ASCII_ALPHANUMERIC | "_")* }
fullIdent = { ident ~ ("." ~ ident)* }
messageName = { ident }
enumName = { ident }
fieldName = { ident }
oneofName = { ident }
mapName = { ident }
serviceName = { ident }
rpcName = { ident }
groupName = { capitalLetter ~ (letter | decimalDigit | "_")* }

intLit = { hexLit | octalLit | decimalLit }
decimalLit = @{ ("-" | "+")? ~ ('1'..'9') ~ decimalDigit* }
octalLit = @{ ("-" | "+")? ~ "0" ~ octalDigit* }
hexLit = @{ ("-" | "+")? ~ ("0x" | "0X") ~ hexDigit+ }

floatLit = @{ ("-" | "+")? ~ ((decimalDigit+ ~ "." ~ decimalDigit* ~ exponent?) | (decimalDigit+ ~ exponent) | ("." ~ decimalDigit+ ~ exponent?)) | "inf" | "nan" }
decimals = { ("-" | "+")? ~ decimalDigit+ }
exponent = { ("e" | "E") ~ ("+" | "-")? ~ decimals }

boolLit = { "true" | "false" }

strLit = { ( single_string | double_string )+ }
single_string = @{ "'" ~ single_string_char* ~ "'" }
double_string = @{ "\"" ~ double_string_char* ~ "\"" }
single_string_char = { escape_sequence | single_quote | !("'" | NEWLINE | "\\") ~ ANY }
double_string_char = { escape_sequence | double_quote | !("\"" | NEWLINE | "\\") ~ ANY }
single_quote = @{ "\\'" }
double_quote = @{ "\\\"" }

escape_sequence = @{ hexEscape | octEscape | charEscape | unicodeEscape | unicodeLongEscape }
hexEscape = { "\\" ~ ("x" | "X") ~ hexDigit ~ hexDigit? }
octEscape = { "\\" ~ octalDigit ~ octalDigit? ~ octalDigit? }
charEscape = { "\\" ~ ("a" | "b" | "f" | "n" | "r" | "t" | "v" | "x" | "?" | "0" | "\\" | "\"" | "'") }
unicodeEscape = { "\\" ~ "u" ~ hexDigit{4} }
unicodeLongEscape = { "\\" ~ "U" ~ ("000" ~ hexDigit{5} | "0010" ~ hexDigit{4}) }

constant = { fullIdent | ((("-" | "+")? ~ floatLit) | ("-" | "+")? ~ intLit) | strLit | boolLit | MessageValue }
arrayConstant = { "[" ~ (constant ~ ("," ~ constant)*)? ~ "]" }
MessageValue = { ident? ~ "{" ~ ((fieldAssignment | MessageValue) ~ ","?)* ~ "}" }

emptyStatement = { ";" }
"##]
pub struct ProtoParser;
