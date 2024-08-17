//! The purpose of the parser is to parse the token vector and return abstract syntax tree.
//! The token vectors is generated by the scanner
use log::{debug, error, info, trace, warn};

use super::parse_tree_unfinished::{
    ParseTreeUnfinshed, PatternMatchingRes, RepetitivePatternMatchingRes,
};
use crate::err_lox::*;
use crate::interpreter::scanner;
use crate::interpreter::token::{self, Token, TokenArcVec, TokenType};
use crate::interpreter::AST_Node::*;
use std::error::Error;
use std::sync::{Arc, Mutex};

// delete_consec_stmt_sep_from_idx_inclusive(tree, index) delete all consective stmt_sep starting
// from tree[index], inclusively, and returns the number of nodes deleted
// If tree[index] is not stmt_sep, it do nothing.
//
// the number of removed index is removed from $len
// TODO: move this macro into a function
#[macro_export]
macro_rules! delete_stmt_sep_adjust_len {
    ($tree:expr, $idx:expr, $len:expr) => {
        $len -= crate::interpreter::parser::delete_consec_stmt_sep_from_idx_inclusive($tree, $idx);
    };
}

macro_rules! HandleParseState {
    ($fun:expr) => {
        match $fun {
            ParseState::Finished => {}
            ParseState::Err(err) => return ParseState::Err(err),
            ParseState::Unfinished => {}
        }
    };
}

// TODO: get better name
macro_rules! handle_result_none_errorlox_for_parsestate {
    ($res:expr) => {
        match $res {
            Err(e) => {
                return ParseState::Err(e);
            }
            _ => {}
        }
    };
}

lazy_static! {
    static ref RVALUES: Vec<AST_Type> =
        [AST_Type::get_all_expr(), vec![AST_Type::Identifier]].concat();
    static ref COPULATIVE: Vec<AST_Type> = Vec::from([
        AST_Type::Unparsed(TokenType::PLUS),
        AST_Type::Unparsed(TokenType::PLUS_EQUAL),
        AST_Type::Unparsed(TokenType::MINUS),
        AST_Type::Unparsed(TokenType::MINUS_EQUAL),
        AST_Type::Unparsed(TokenType::STAR),
        AST_Type::Unparsed(TokenType::STAR_EQUAL),
        AST_Type::Unparsed(TokenType::SLASH),
        AST_Type::Unparsed(TokenType::SLASH_EQUAL),
        AST_Type::Unparsed(TokenType::EQUAL),
        AST_Type::Unparsed(TokenType::EQUAL_EQUAL),
        AST_Type::Unparsed(TokenType::BANG_EQUAL),
        AST_Type::Unparsed(TokenType::GREATER),
        AST_Type::Unparsed(TokenType::GREATER_EQUAL),
        AST_Type::Unparsed(TokenType::LESS),
        AST_Type::Unparsed(TokenType::LESS_EQUAL),
    ]);
}

#[derive(Debug)]
pub enum ParseState {
    Finished,
    Unfinished,
    Err(ErrorLox),
}

/// Public API for parsing the tree
/// tree: parsed (may be unfinished)
/// tokens: more tokens to be parsed, which will be appended to the end of the tree
// parse the input strings into tokens, then feed the token into the unfinished parse tree, which
// is parsed
pub fn parse(tree: &mut ParseTreeUnfinshed, source: &str) -> ParseState {
    let contents = std::fs::read_to_string(source).expect("Should have been able to read the file");

    let mut line_number = 1;
    let tokens: TokenArcVec = match scanner::scan_tokens(&contents, &mut line_number, source) {
        Ok(ok) => ok,
        Err(e) => {
            return ParseState::Err(e);
        }
    };

    let input_list = ParseTreeUnfinshed::from(&tokens);
    info!("Input List:\n{:?}\n", input_list);
    tree.extend(input_list);

    real_parse(tree)
}

// this is the real parse. Define here for recursion
fn real_parse(tree: &mut ParseTreeUnfinshed) -> ParseState {
    if tree.len() <= 1 {
        return ParseState::Finished;
    }

    HandleParseState!(parse_parenthesis(tree));
    HandleParseState!(parse_braces(tree));
    HandleParseState!(parse_function_definition(tree));
    HandleParseState!(parse_function_eval(tree));

    HandleParseState!(parse_prefix(
        tree,
        vec![AST_Type::Unparsed(TokenType::MINUS)],
        [AST_Type::get_all_expr(), vec![AST_Type::Identifier,],].concat(),
        [
            AST_Type::get_all_stmt(),
            vec![AST_Type::Unparsed(TokenType::STMT_SEP),],
            COPULATIVE.clone()
        ]
        .concat(),
        AST_Type::Expr(ExprType::Negated),
    ));

    let plus_minus_ternery_valid_types =
        [AST_Type::get_all_expr(), vec![AST_Type::Identifier]].concat();

    // parse times, divide, and modular
    HandleParseState!(parse_ternary_left_assoc(
        tree,
        &RVALUES,
        &vec![TokenType::STAR, TokenType::SLASH, TokenType::PERCENT],
        &plus_minus_ternery_valid_types,
        AST_Type::Expr(ExprType::Normal),
    ));
    // parse plus minus
    HandleParseState!(parse_ternary_left_assoc(
        tree,
        &plus_minus_ternery_valid_types,
        &vec![TokenType::PLUS, TokenType::MINUS],
        &plus_minus_ternery_valid_types,
        AST_Type::Expr(ExprType::Normal),
    ));

    // parse >=, <=, >, <
    HandleParseState!(parse_ternary_left_assoc(
        tree,
        &plus_minus_ternery_valid_types,
        &vec![
            TokenType::GREATER,
            TokenType::GREATER_EQUAL,
            TokenType::LESS,
            TokenType::LESS_EQUAL
        ],
        &plus_minus_ternery_valid_types,
        AST_Type::Expr(ExprType::Normal),
    ));

    //parse ==, !=
    HandleParseState!(parse_ternary_left_assoc(
        tree,
        &plus_minus_ternery_valid_types,
        &vec![TokenType::EQUAL_EQUAL, TokenType::BANG_EQUAL],
        &plus_minus_ternery_valid_types,
        AST_Type::Expr(ExprType::Normal),
    ));

    // println!("Before ASSIGNMENT:\n{tree:?}\nEND");
    // parsing assignment a = 2;
    HandleParseState!(parse_assignment_like(
        tree,
        vec![AST_Type::Unparsed(TokenType::EQUAL)],
        AST_Type::Stmt(StmtType::Assignment),
    ));

    // parse a += 1
    HandleParseState!(parse_assignment_like(
        tree,
        vec![AST_Type::Unparsed(TokenType::PLUS_EQUAL),],
        AST_Type::Stmt(StmtType::PlusEqual),
    ));

    // parse a -= 1
    HandleParseState!(parse_assignment_like(
        tree,
        vec![AST_Type::Unparsed(TokenType::MINUS_EQUAL)],
        AST_Type::Stmt(StmtType::MinusEqual),
    ));

    // a *=1
    HandleParseState!(parse_assignment_like(
        tree,
        vec![AST_Type::Unparsed(TokenType::STAR_EQUAL)],
        AST_Type::Stmt(StmtType::StarEqual),
    ));

    // a /= 1
    HandleParseState!(parse_assignment_like(
        tree,
        vec![AST_Type::Unparsed(TokenType::SLASH_EQUAL)],
        AST_Type::Stmt(StmtType::SlashEqual),
    ));

    HandleParseState!(parse_assignment_like(
        tree,
        vec![AST_Type::Unparsed(TokenType::PERCENT_EQUAL)],
        AST_Type::Stmt(StmtType::PercentEqual),
    ));

    // parse var a = b;
    HandleParseState!(parse_prefix(
        tree,
        vec![AST_Type::Unparsed(TokenType::VAR)],
        vec![AST_Type::Stmt(StmtType::Assignment)],
        [
            AST_Type::get_all_stmt(),
            vec![AST_Type::Unparsed(TokenType::STMT_SEP)]
        ]
        .concat(),
        AST_Type::Stmt(StmtType::Declaration),
    ));

    HandleParseState!(parse_comma(tree, &RVALUES, AST_Type::Tuple,));

    HandleParseState!(parse_ternary_stmt_like_while(
        tree,
        &vec![AST_Type::Unparsed(TokenType::WHILE)],
        &vec![
            AST_Type::Expr(ExprType::Paren),
            AST_Type::Expr(ExprType::Normal)
        ],
        &vec![AST_Type::Stmt(StmtType::Braced)],
        AST_Type::Stmt(StmtType::While),
        "Expected expression after while",
        "Expected {stmt} after while",
    ));

    HandleParseState!(parse_if(tree));

    HandleParseState!(parse_stmt_sep(tree));
    HandleParseState!(parse_stmt_into_compound_stmt(tree));

    tree.is_finished()
}

/// TODO: rename
///
/// get the index of the next valid token, starting from tree [index]
/// if tree[index] is valid, return index
/// only stmt_sep are invalid token
fn get_next_valid_node(tree: &ParseTreeUnfinshed, index: usize) -> Option<usize> {
    let length = tree.len();
    let mut tmp = index;
    while tmp < length {
        if AST_Node::get_AST_Type_from_arc(tree[tmp].clone())
            != AST_Type::Unparsed(TokenType::STMT_SEP)
        {
            return Some(tmp);
        }
        tmp += 1;
    }

    None
}

/// Delete all consective stmt_sep starting from index onwards. Return number of deleted nodes
pub(crate) fn delete_consec_stmt_sep_from_idx_inclusive(
    tree: &mut ParseTreeUnfinshed,
    idx: usize,
) -> usize {
    let upper = match get_next_valid_node(tree, idx) {
        None => tree.len(),
        Some(num) => num,
    };
    for _ in idx..upper {
        tree.remove(idx);
    }

    upper - idx
}

/// trying to find the matching location for left ... right. left token and right token shall
/// behave like parenthesis. return a vector of tuple (start, end) holding the index of all of the
/// outermost matching delimiter
/// if there is no left or right exist at all, return vec of length 0
/// example:
/// tree = [], left = (, right = )
/// return Vec::new()
///
/// tree = (1), left = (, right = )
/// return [(0, 2)]
///
/// tree = 1 + (1 + 2), left = (, right = )
/// return [(2, 6)]
///
/// tree = 1 + (1 + 2) + (3+ (2+3)), left = (, right = )
/// return [(2, 6), (8, 16)]
pub(crate) fn get_delimiter_location(
    left: AST_Type,
    right: AST_Type,
    tree: &ParseTreeUnfinshed,
) -> Result<Vec<(usize, usize)>, ErrorLox> {
    let mut ret: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    let mut count = 0;
    let left = left.clone();
    for i in 0..tree.len() {
        match AST_Node::get_AST_Type_from_arc(tree[i].clone()) {
            y if y == left => {
                if count == 0 {
                    start = i;
                }
                count += 1;
            }
            y if y == right => {
                count -= 1;
                if count == 0 {
                    ret.push((start, i));
                }
                if count < 0 {
                    let e =
                        ErrorLox::from_arc_mutex_ast_node(tree[i].clone(), "Extra right delimiter");
                    return Err(e);
                }
            }
            _ => {}
        }
    }

    if count > 0 {
        let mut e = ErrorLox::from_arc_mutex_ast_node(
            tree[tree.len() - 1].clone(),
            &format!("Unpaired {:?}", left),
        );
        e.set_error_type(ErrorType::UnterminatedDelimiter);

        // println!("{e:?}");

        return Err(e);
    }

    Ok(ret)
}

// recursively parse parenthesis
fn parse_parenthesis(tree: &mut ParseTreeUnfinshed) -> ParseState {
    let locations = match get_delimiter_location(
        AST_Type::Unparsed(TokenType::LEFT_PAREN),
        AST_Type::Unparsed(TokenType::RIGHT_PAREN),
        &tree,
    ) {
        Ok(ok) => ok,
        // left and right paren must be at the same line
        Err(e) => return ParseState::Err(e),
    };

    // recursive call;
    for (start, end) in locations.into_iter().rev() {
        let mut slice = tree.slice(start + 1, end);
        let sup_parse = real_parse(&mut slice);
        match sup_parse {
            ParseState::Err(e) => return ParseState::Err(e),
            ParseState::Unfinished => {
                return ParseState::Err(ErrorLox::from_arc_mutex_ast_node(
                    tree[start].clone(),
                    "Incomplete Inner Expr",
                ));
            }
            ParseState::Finished => {
                let res = match slice.get_finished_node() {
                    Ok(ok) => ok,
                    Err(e) => return ParseState::Err(e),
                };
                // the parse result may be none
                match res {
                    // in such case the parenethesis is just by itself
                    None => {
                        tree.remove(end);
                        AST_Node::set_arc_mutex_AST_Type(
                            tree[start].clone(),
                            AST_Type::Expr(ExprType::Paren),
                        );
                    }
                    Some(result) => {
                        for _ in (start + 1)..=(end) {
                            tree.remove(start + 1);
                        }
                        // tree.replace(start, result);
                        AST_Node::set_arc_mutex_AST_Type(
                            tree[start].clone(),
                            AST_Type::Expr(ExprType::Paren),
                        );
                        AST_Node::arc_mutex_append_child(tree[start].clone(), result);
                    }
                }
            }
        }
    }
    ParseState::Finished
}

fn parse_braces(tree: &mut ParseTreeUnfinshed) -> ParseState {
    let locations = match get_delimiter_location(
        AST_Type::Unparsed(TokenType::LEFT_BRACE),
        AST_Type::Unparsed(TokenType::RIGHT_BRACE),
        &tree,
    ) {
        Ok(ok) => {
            // println!("{ok:?}");
            ok
        }
        Err(e) => {
            return ParseState::Err(e);
        }
    };

    // recursive call;
    for (start, end) in locations.into_iter().rev() {
        let mut slice = tree.slice(start + 1, end);
        // println!("SLICES: \n{:?}\nEND", slice);

        let sup_parse = real_parse(&mut slice);
        // println!("SLICES after parse: \n{:?}\nEND", slice);
        match sup_parse {
            ParseState::Err(e) => return ParseState::Err(e),
            ParseState::Unfinished => {
                return ParseState::Err(ErrorLox::from_arc_mutex_ast_node(
                    tree[start].clone(),
                    "Incomplete Inner Expr",
                ));
            }
            ParseState::Finished => {}
        };

        // we have a success sub parse
        let res = match slice.get_finished_node() {
            Ok(ok) => ok,
            Err(e) => return ParseState::Err(e),
        };
        // the parse result may be none
        // If there is result, the result is presented by one compound
        // stmt, which is redundant
        match res {
            // in such case the parenethesis is just by itself
            None => {
                tree.remove(end);
                AST_Node::set_arc_mutex_AST_Type(
                    tree[start].clone(),
                    AST_Type::Stmt(StmtType::Braced),
                );
            }
            Some(result) => {
                for _ in (start + 1)..=(end) {
                    tree.remove(start + 1);
                }
                // tree.replace(start, result);
                AST_Node::set_arc_mutex_AST_Type(
                    tree[start].clone(),
                    AST_Type::Stmt(StmtType::Braced),
                );
                // if the result is a compound stmt: deconstruct the compound stmt
                // and append its children to start
                if AST_Node::get_AST_Type_from_arc(result.clone())
                    == AST_Type::Stmt(StmtType::Compound)
                {
                    AST_Node::arc_mutex_append_children(
                        tree[start].clone(),
                        &AST_Node::arc_mutex_get_children(result.clone()),
                    );
                } else {
                    AST_Node::arc_mutex_append_child(tree[start].clone(), result)
                }
            }
        }
    }
    ParseState::Finished
}

fn parse_function_eval(tree: &mut ParseTreeUnfinshed) -> ParseState {
    let mut i = 0;
    let mut length = tree.len();

    while i < length {
        if AST_Node::get_AST_Type_from_arc(tree[i].clone()) != AST_Type::Identifier {
            i += 1;
            continue;
        }
        delete_stmt_sep_adjust_len!(tree, i + 1, length);

        // tree[i] is identifier, tree[i+1] is a valid node
        if i + 1 < length
            && AST_Node::get_AST_Type_from_arc(tree[i + 1].clone())
                == AST_Type::Expr(ExprType::Paren)
        {
            // tree[i + 1] is paren.
            // The paren struct itself shall hold at most one child, and thus is redundant
            // and shall be removed.
            AST_Node::arc_mutex_append_child(tree[i].clone(), tree[i + 1].clone());
            AST_Node::set_arc_mutex_AST_Type(tree[i].clone(), AST_Type::Expr(ExprType::Function));
            tree.remove(i + 1);
            length -= 1;
        }
        i += 1;
    }
    ParseState::Finished
}

fn parse_function_definition(tree: &mut ParseTreeUnfinshed) -> ParseState {
    let mut i = 0;
    let mut length = tree.len();
    let patterns = vec![
        vec![AST_Type::Unparsed(TokenType::FN)],
        vec![AST_Type::Identifier],
        vec![AST_Type::Expr(ExprType::Paren)],
        vec![AST_Type::Stmt(StmtType::Braced)],
    ];
    while i < length {
        match tree.match_ast_pattern(i, &patterns, 0) {
            PatternMatchingRes::Nomatch => {}
            PatternMatchingRes::FailedAt(num) => {
                let mut length = i + num;
                if length >= tree.len() {
                    length = tree.len() - 1;
                }
                return ParseState::Err(ErrorLox::from_arc_mutex_ast_node(
                    tree[length].clone(),
                    &format!("Expected {:?}", patterns[num]),
                ));
            }
            PatternMatchingRes::Matched => {
                AST_Node::arc_mutex_append_child(tree[i].clone(), tree[i + 1].clone());
                AST_Node::arc_mutex_append_child(tree[i].clone(), tree[i + 2].clone());
                AST_Node::arc_mutex_append_child(tree[i].clone(), tree[i + 3].clone());
                AST_Node::set_arc_mutex_AST_Type(
                    tree[i].clone(),
                    AST_Type::Stmt(StmtType::FunctionDef),
                );

                tree.remove(i + 1);
                tree.remove(i + 1);
                tree.remove(i + 1);
                length -= 3;
            }
        }
        i += 1;
    }

    ParseState::Finished
}

// TODO: REFACTOR WITH AST_MATCH

/// This function constructs the ternary left associtive operators into tree, whose grammer is
/// similar to +, -, *, /
///
/// If the operator is found, but left_ast_types or right_ast_types are not found,  
fn parse_ternary_left_assoc(
    tree: &mut ParseTreeUnfinshed,
    left_ast_types: &[AST_Type],
    operator_token_types: &[TokenType],
    right_ast_types: &[AST_Type],
    result_type: AST_Type,
) -> ParseState {
    // if the operator appears at the beginning or at the end, return error.
    if AST_Node::arc_belongs_to_Token_type(tree[0].clone(), operator_token_types) {
        return ParseState::Err(ErrorLox::from_arc_mutex_ast_node(
            tree[0].clone(),
            &format!("{:?} without preceding expression", operator_token_types),
        ));
    }
    if AST_Node::arc_belongs_to_Token_type(tree[tree.len() - 1].clone(), operator_token_types) {
        return ParseState::Err(ErrorLox::from_arc_mutex_ast_node(
            tree[tree.len() - 1].clone(),
            &format!("{:?} without following expression", operator_token_types),
        ));
    }

    // Start of parsing

    let mut length = tree.len();
    let mut i = 0;

    // ignore the last two tokens
    while i + 2 < length {
        if !AST_Node::arc_belongs_to_Token_type(tree[i + 1].clone(), operator_token_types) {
            i += 1;
            continue;
        }
        // match the type of the first token
        if !AST_Node::arc_belongs_to_AST_type(tree[i].clone(), left_ast_types) {
            i += 1;
            continue;
        }

        // check the third toklen
        handle_result_none_errorlox_for_parsestate!(tree.error_handle_tree_i_is_in_types(
            i + 2,
            right_ast_types,
            ""
        ));

        // Construct the tree
        {
            let mut root = tree[i + 1].lock().unwrap();
            root.set_AST_Type(result_type.clone());
            root.append_child(tree[i].clone());
            root.append_child(tree[i + 2].clone());
        }
        // remove the first expr,
        // note the length of the array decreases by one
        tree.remove(i);
        // remove the second expr
        tree.remove(i + 1);
        length -= 2;
        // skipping i += 1; the new node needs to be parsed again
    }
    ParseState::Finished
}

/// parse statements like expr; into stmt(normal) -> expr
fn parse_post_single(
    tree: &mut ParseTreeUnfinshed,
    ast_type: &[AST_Type],
    operator_token_type: &[TokenType],
    result_type: AST_Type,
) -> ParseState {
    let mut length = tree.len();
    let mut i = 0;

    while i + 1 < length {
        if !AST_Node::arc_belongs_to_AST_type(tree[i].clone(), ast_type) {
            i += 1;
            continue;
        }
        if !AST_Node::arc_belongs_to_Token_type(tree[i + 1].clone(), operator_token_type) {
            i += 1;
            continue;
        }
        let node = tree[i + 1].clone();
        let mut node = node.lock().unwrap();
        node.set_AST_Type(result_type.clone());
        node.append_child(tree[i].clone());
        tree.remove(i);
        length -= 1;
        i += 1;
    }
    ParseState::Finished
}

/// The final, finished parse tree shall consist of a single root of type Stmt(Compound). All of
/// the substatment shall be children of this node.
/// This function arrange vector of statement into one node.
/// It creates an empty compound node at first and scans the tree
/// If found a lone statement, the lone statement is appended into the compound node. If found a
/// compound statement, two compound are merge. The compound statement was then inserted into the
/// tree properly
/// If expressions are found, they are left alone. This is important, as the result of parsing
/// could be an expression and not a statement
fn parse_stmt_into_compound_stmt(tree: &mut ParseTreeUnfinshed) -> ParseState {
    let mut length = tree.len();
    let mut i = 0;

    let mut compound: Arc<Mutex<AST_Node>> =
        AST_Node::new(AST_Type::Stmt(StmtType::Compound), Token::dummy()).into();

    while i < length {
        if AST_Node::is_arc_mutex_stmt(tree[i].clone()) {
            let has_child: bool = AST_Node::arc_mutex_has_children(compound.clone());
            if AST_Node::is_arc_mutex_compound_stmt(tree[i].clone()) {
                let node = tree[i].clone();
                let node = node.lock().unwrap();
                for i in node.get_children() {
                    AST_Node::arc_mutex_append_child(compound.clone(), i.clone());
                }
            } else {
                AST_Node::arc_mutex_append_child(compound.clone(), tree[i].clone());
            }
            if has_child {
                tree.remove(i);
                length -= 1;
            } else {
                tree.replace(i, compound.clone());
                i += 1;
            }
        } else {
            // in such case the node is expr
            i += 1;
            if AST_Node::arc_mutex_has_children(compound.clone()) {
                compound = AST_Node::new(AST_Type::Stmt(StmtType::Compound), Token::dummy()).into();
            }
        }
    }

    ParseState::Finished
}

/// parse statements like
/// ```a = 1+2``` (identifier, equal, expr, stmt_sep)
fn parse_assignment_like(
    tree: &mut ParseTreeUnfinshed,
    key_ast_type: Vec<AST_Type>,
    result_type: AST_Type,
) -> ParseState {
    let mut i = 0;
    let mut length = tree.len();
    let expected = vec![
        vec![AST_Type::Identifier],
        // vec![AST_Type::Unparsed(TokenType::EQUAL)],
        key_ast_type,
        RVALUES.clone(),
        vec![AST_Type::Unparsed(TokenType::STMT_SEP)],
    ];
    // println!("Assignment like: \n{:?}", tree);
    while i < length {
        let res = tree.match_ast_pattern(i, &expected, 1);

        match res {
            PatternMatchingRes::Matched => {
                {
                    let mut root = tree[i + 1].lock().unwrap();
                    root.set_AST_Type(result_type.clone());
                    root.append_child(tree[i].clone());
                    root.append_child(tree[i + 2].clone());
                }
                // remove the first expr,
                // note the length of the array decreases by one
                tree.remove(i);
                // remove the second expr
                tree.remove(i + 1);
                tree.remove(i + 1);
                length -= 3;
            }
            PatternMatchingRes::FailedAt(num) => {
                let mut length = i + num;
                if length >= tree.len() {
                    length = tree.len() - 1;
                }
                return ParseState::Err(ErrorLox::from_arc_mutex_ast_node(
                    tree[length].clone(),
                    &format!("Expected {:?}", expected[num]),
                ));
            }
            _ => {}
        }
        i += 1;
    }

    ParseState::Finished
}

fn parse_prefix(
    tree: &mut ParseTreeUnfinshed,
    prefix_types: Vec<AST_Type>,
    sequential_type: Vec<AST_Type>,
    valid_after_ast_type: Vec<AST_Type>,
    result_type: AST_Type,
) -> ParseState {
    let mut i = 0;
    let mut length = tree.len();
    let expected = vec![prefix_types, sequential_type];

    while i < length {
        // Only parse if it is prefix
        if i > 0 {
            if !AST_Node::arc_belongs_to_AST_type(tree[i - 1].clone(), &valid_after_ast_type) {
                i += 1;
                continue;
            }
        }
        let res = tree.match_ast_pattern(i, &expected, 0);

        match res {
            PatternMatchingRes::Nomatch => {}
            PatternMatchingRes::FailedAt(num) => {
                let mut length = i + num;
                if length >= tree.len() {
                    length = tree.len() - 1;
                }
                return ParseState::Err(ErrorLox::from_arc_mutex_ast_node(
                    tree[length].clone(),
                    &format!("Expected {:?}", expected[num]),
                ));
            }
            PatternMatchingRes::Matched => {
                let node = tree[i + 1].clone();
                let wrapper = Arc::new(Mutex::new(AST_Node::new_wrapper_node(node.clone())));
                AST_Node::set_arc_mutex_AST_Type(wrapper.clone(), result_type.clone());
                tree[i + 1] = wrapper;
                tree.remove(i); // remove the prefix
                length -= 1;
            }
        }
        i += 1;
    }

    ParseState::Finished
}

fn parse_stmt_sep(tree: &mut ParseTreeUnfinshed) -> ParseState {
    let mut i = 0;
    let mut length = tree.len();

    while i < length {
        if AST_Node::get_AST_Type_from_arc(tree[i].clone())
            == AST_Type::Unparsed(TokenType::STMT_SEP)
        {
            if i == 0 {
                tree.remove(i);
                length -= 1;
                continue;
            }
            if AST_Node::is_arc_mutex_stmt(tree[i - 1].clone()) {
                tree.remove(i);
                length -= 1;
                continue;
            }
            // TODO: IS THIS HANDLING WHAT WE EXPECTED?
            let node = tree[i].clone();
            let mut node = node.lock().unwrap();
            node.set_AST_Type(AST_Type::Stmt(StmtType::Normal));
            node.append_child(tree[i - 1].clone());
            tree.remove(i - 1);
            length -= 1;
            continue;
            // return ParseState::Err(ErrorLox::from_arc_mutex_ast_node(
            //     tree[i - 1].clone(),
            //     "Unexpected Node, likely an internal error.",
            //     source,
            // ));
        }
        i += 1;
    }
    ParseState::Finished
}

/// Parse syntax like `while expr {stmt}`
/// Error Handling:
/// Err:: expected expressiong if
/// while \n expr {stmt} or while {stmt} or while (by itself)
/// Err:: expected {stmt} if
/// while expr or while expr stmt
fn parse_ternary_stmt_like_while(
    tree: &mut ParseTreeUnfinshed,
    operator_token_types: &[AST_Type],
    left_ast_types: &[AST_Type],
    right_ast_types: &[AST_Type],
    result_type: AST_Type,
    error_1: &str,
    error_2: &str,
) -> ParseState {
    let mut length = tree.len();
    let mut i = 0;
    // ignore the last two tokens
    while i + 2 < length {
        // match the type of the first token
        if !AST_Node::arc_belongs_to_AST_type(tree[i].clone(), operator_token_types) {
            i += 1;
            continue;
        }
        delete_consec_stmt_sep_from_idx_inclusive(tree, i + 1);

        handle_result_none_errorlox_for_parsestate!(tree.error_handle_tree_i_is_in_types(
            i + 1,
            left_ast_types,
            error_1
        ));

        delete_consec_stmt_sep_from_idx_inclusive(tree, i + 2);
        // check the third toklen
        handle_result_none_errorlox_for_parsestate!(tree.error_handle_tree_i_is_in_types(
            i + 2,
            right_ast_types,
            error_2
        ));

        // if !AST_Node::arc_belongs_to_AST_type(tree[i + 2].clone(), right_ast_types) {
        //     return ParseState::Err(ErrorLox::from_arc_mutex_ast_node(tree[i].clone(), error_2));
        // }
        
        // Construct the tree
        {
            let mut root = tree[i].lock().unwrap();
            root.set_AST_Type(result_type.clone());
            root.append_child(tree[i + 1].clone());
            root.append_child(tree[i + 2].clone());
        }
        // remove the extra nodes,
        // NOTE: the length of the array decreases by two
        tree.remove(i + 1);
        tree.remove(i + 1);
        length -= 2;
    }
    ParseState::Finished
}

fn parse_comma(
    tree: &mut ParseTreeUnfinshed,
    expected_types: &[AST_Type],
    result_type: AST_Type,
) -> ParseState {
    let mut i = 0;
    let mut length = tree.len();

    while i < length {
        if AST_Node::get_AST_Type_from_arc(tree[i].clone()) != AST_Type::Unparsed(TokenType::COMMA)
        {
            i += 1;
            continue;
        }

        //NOTE: now tree[i] is comma

        // need to handle the cases with multiple commas like
        // 1, 2, 3, 4
        let mut comma_count = 0;
        while (i + comma_count * 2) < length
            && AST_Node::get_AST_Type_from_arc(tree[i + comma_count * 2].clone())
                == AST_Type::Unparsed(TokenType::COMMA)
        {
            comma_count += 1;
            if i + (comma_count - 1) * 2 == 0
                || !AST_Node::arc_belongs_to_AST_type(
                    tree[i + (comma_count - 1) * 2 - 1].clone(),
                    expected_types,
                )
            {
                let error_idx = match i + (comma_count - 1) * 2 {
                    tmp if tmp == 0 => 0,
                    tmp => tmp - 1,
                };
                return ParseState::Err(ErrorLox::from_arc_mutex_ast_node(
                    tree[error_idx].clone(),
                    &format!("Expected {expected_types:?}"),
                ));
            }
            if i + (comma_count - 1) * 2 == length - 1
                || !AST_Node::arc_belongs_to_AST_type(
                    tree[i + (comma_count - 1) * 2 + 1].clone(),
                    expected_types,
                )
            {
                let error_idx = match i + (comma_count - 1) * 2 {
                    tmp if tmp == length - 1 => 0,
                    tmp => tmp + 1,
                };
                return ParseState::Err(ErrorLox::from_arc_mutex_ast_node(
                    tree[error_idx].clone(),
                    &format!("Expected {expected_types:?}"),
                ));
            }
        }

        for j in 0..=comma_count {
            AST_Node::arc_mutex_append_child(tree[i].clone(), tree[i - 1 + j * 2].clone())
        }
        AST_Node::set_arc_mutex_AST_Type(tree[i].clone(), result_type.clone());

        for j in ((i + 1)..=(i + comma_count * 2 - 1)).rev() {
            tree.remove(j);
            length -= 1;
        }
        tree.remove(i - 1);
        length -= 1;

        i += 1;
    }

    ParseState::Finished
}

fn parse_if(tree: &mut ParseTreeUnfinshed) -> ParseState {
    let mut root_idx: usize;
    let mut i = 0;
    let mut length = tree.len();

    while i < length {
        if AST_Node::get_AST_Type_from_arc(tree[i].clone()) != AST_Type::Unparsed(TokenType::IF) {
            i += 1;
            continue;
        }

        // NOTE:: tree[i] is if
        delete_stmt_sep_adjust_len!(tree, i + 1, length);
        handle_result_none_errorlox_for_parsestate!(tree.error_handle_tree_i_is_in_types(
            i + 1,
            &RVALUES,
            "Expected rvalues after if"
        ));

        // after RVALUE, braced stmt may be on the new line
        delete_stmt_sep_adjust_len!(tree, i + 2, length);
        handle_result_none_errorlox_for_parsestate!(tree.error_handle_tree_i_is_in_types(
            i + 2,
            &vec![AST_Type::Stmt(StmtType::Braced)],
            "Expected braced stmt after if"
        ));

        // parse if (RVALUE) {STMT}
        // disregarding else and else if for now
        root_idx = i;
        {
            let mut root = tree[i].lock().unwrap();
            root.set_AST_Type(AST_Type::Stmt(StmtType::If));
            root.append_child(tree[i + 1].clone());
            root.append_child(tree[i + 2].clone());
        }
        // remove the extra nodes,
        // note the length of the array decreases by two
        tree.remove(i + 1);
        tree.remove(i + 1);
        length -= 2;
        // i is incremented
        i += 1;

        // get next valid statemnet before continuing parsing
        delete_stmt_sep_adjust_len!(tree, i, length);
        // NOTE: tree[i] may not be else
        // Finishing parsing pattern:
        // if (RVALUE) {STMT}
        let mut num_of_elif_stmt: usize = 0;
        let (res, delta) = tree.match_ast_repetitive_pattern(
            i,
            &vec![
                vec![AST_Type::Unparsed(TokenType::ELSE)],
                vec![AST_Type::Unparsed(TokenType::IF)],
                RVALUES.clone(),
                vec![AST_Type::Stmt(StmtType::Braced)],
            ],
        );
        length -= delta;
        // println!("res: {res:?}");
        match res {
            // in such case there is no else
            RepetitivePatternMatchingRes::Nomatch => {
                // next iteratioh
                // i is already incremented
                continue;
            }
            RepetitivePatternMatchingRes::MatchUntil(num) => match num % 4 {
                0 => {
                    num_of_elif_stmt = (num + 1) / 4;
                }
                1 => {
                    // matched else and if
                    return ParseState::Err(ErrorLox::from_arc_mutex_ast_node(
                        tree[i + 1].clone(),
                        &format!("Expected RVALUES after if",),
                    ));
                }
                2 => {
                    return ParseState::Err(ErrorLox::from_arc_mutex_ast_node(
                        tree[i + 1].clone(),
                        &format!("Expected braced stmt after if",),
                    ));
                }
                3 => {
                    num_of_elif_stmt = (num + 1) / 4;
                }
                _ => {}
            },
        }

        // construct the tree
        for _ in 0..num_of_elif_stmt {
            // iterate thorugh each of the else if stmt
            // tree[i] is else in `else if`, remove it
            tree.remove(i);
            // NOTE: now tree[i] is if
            AST_Node::arc_mutex_append_child(tree[i].clone(), tree[i + 1].clone());
            AST_Node::arc_mutex_append_child(tree[i].clone(), tree[i + 2].clone());
            AST_Node::set_arc_mutex_AST_Type(tree[i].clone(), AST_Type::Stmt(StmtType::Elseif));
            AST_Node::arc_mutex_append_child(tree[root_idx].clone(), tree[i].clone());
            // remove the extra nodes,
            // note the length of the array decreases by two
            tree.remove(i);
            tree.remove(i);
            tree.remove(i);
            length -= 4;
        }

        // check the dangling else
        delete_stmt_sep_adjust_len!(tree, i, length);
        if i >= length
            || AST_Node::get_AST_Type_from_arc(tree[i].clone())
                != AST_Type::Unparsed(TokenType::ELSE)
        {
            // next iteration
            // i += 1;
            continue;
        }

        // now tree[i] is else,
        // error!("ANUOINNONN ELSE: {:?}", tree[i]);
        // we expect else {}
        delete_stmt_sep_adjust_len!(tree, i, length);
        handle_result_none_errorlox_for_parsestate!(tree.error_handle_tree_i_is_in_types(
            i + 1,
            &vec![AST_Type::Stmt(StmtType::Braced)],
            "Expected rvalues after if"
        ));

        // handle else
        AST_Node::arc_mutex_append_child(tree[i].clone(), tree[i + 1].clone());
        AST_Node::set_arc_mutex_AST_Type(tree[i].clone(), AST_Type::Stmt(StmtType::Else));
        AST_Node::arc_mutex_append_child(tree[root_idx].clone(), tree[i].clone());

        tree.remove(i + 1);
        tree.remove(i);
        length -= 2;
        // is it ?
        i += 2;
    }
    ParseState::Finished
}
