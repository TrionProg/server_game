use std::str::Chars;

use std::collections::BTreeMap;
use std::collections::btree_map::Entry::{Occupied, Vacant};

use std::str::FromStr;
use std::slice::Iter;

use lexer::{Lexeme, Cursor};

pub enum ParameterValue {
    String{line:usize, value:String},
    List(List),
    Map(Map),
}

impl ParameterValue {
    pub fn getString<'a>( &'a self ) -> Result<&'a String,String>{
        match *self {
            ParameterValue::String{ line, ref value } =>
                Ok(value),
            ParameterValue::Map( ref map ) =>
                Err(format!("Line {} : Parameter is not string", map.line)),
            ParameterValue::List( ref list ) =>
                Err(format!("Line {} : Parameter is not string", list.line)),
        }
    }

    pub fn getMap<'a>( &'a self ) -> Result<&'a Map,String>{
        match *self {
            ParameterValue::String{ line, ref value } =>
                Err(format!("Line {} : Parameter is not map", line)),
            ParameterValue::Map( ref map ) =>
                Ok( map ),
            ParameterValue::List( ref list ) =>
                Err(format!("Line {} : Parameter is not map", list.line)),
        }
    }

    pub fn getList<'a>( &'a self ) -> Result<&'a List,String>{
        match *self {
            ParameterValue::String{ line, ref value } =>
                Err(format!("Line {} : Parameter is not list", line)),
            ParameterValue::Map( ref map ) =>
                Err(format!("Line {} : Parameter is not list", map.line)),
            ParameterValue::List( ref list ) =>
                Ok( list ),
        }
    }
}

pub struct Map{
    line:usize,
    params:BTreeMap<String, ParameterValue>,
}

impl Map {
    fn parse( cur:&mut Cursor, endsWith:char ) -> Result<Map, String>{
        let mapLine=cur.line;

        let mut params=BTreeMap::new();

        loop {
            match try!( cur.next() ) {
                Lexeme::String( paramName ) => {
                    match try!( cur.next() ) {
                        Lexeme::Eq | Lexeme::Colon => {},
                        _=> return Err( format!("{}Expected : or = , but {} have been found", cur.printLine(), cur.lex.print() )),
                    }

                    let paramValue=match try!( cur.next() ) {
                        Lexeme::Bracket( '{') =>
                            ParameterValue::Map( try!(Map::parse( cur, '}' )) ),
                        Lexeme::Bracket( '[') =>
                            ParameterValue::List( try!(List::parse( cur )) ),
                        Lexeme::String( s ) =>
                            ParameterValue::String{line:cur.line, value: String::from(s) },
                        Lexeme::NewLine | Lexeme::Comma | Lexeme::EOF | Lexeme::Bracket('}') =>
                            return Err( format!("{}Value of parameter has been missed", cur.printLine() ) ),
                        _=>
                            return Err( format!("{}Expected string(\"..\"), list([...]), map(_.._) , but {} have been found", cur.printLine(), cur.lex.print()) ),
                    };

                    match params.entry( String::from(paramName) ) {
                        Vacant( entry ) => {entry.insert( paramValue );},
                        Occupied( e ) => return Err( format!("{}Parameter has been declarated before", cur.printLine()) ),
                    }

                    match try!( cur.next() ){
                        Lexeme::EOF => {
                            if endsWith!='\0' {
                                return Err( format!("{}Expected _, but EOF was found", cur.printLine()) );
                            }

                            break;
                        },
                        Lexeme::Bracket('}') => {
                            if endsWith!='}' {
                                return Err( format!("{}Expected EOF, but _ was found", cur.printLine()) );
                            }

                            break;
                        },
                        Lexeme::NewLine | Lexeme::Comma => {},
                        _=>return Err( format!("{}Expected 'new line ', ',' or {} , but {} have been found", cur.printLine(), endsWith, cur.lex.print()) ),
                    }
                },
                Lexeme::NewLine => {},
                Lexeme::EOF => break,
                _=>return Err(format!("{}Expected parameter name, but {} have been found", cur.printLine(), cur.lex.print()) ),
            }
        }

        Ok(
            Map{
                line:mapLine,
                params:params,
            }
        )
    }

    pub fn getMap<'a>( &'a self, name:&'a str ) -> Result<&'a Map,String>{
        match self.params.get( name ){
            Some( pv ) => {
                match *pv {
                    ParameterValue::String{ line, ref value } =>
                        Err(format!("Line {} : Parameter \"{}\" is not map", line, name)),
                    ParameterValue::Map( ref map ) =>
                        Ok( map ),
                    ParameterValue::List( ref list ) =>
                        Err(format!("Line {} : Parameter \"{}\" is not map", list.line, name)),
                }
            },
            None=>Err(format!("Line {} : Map has no parameter \"{}\"", self.line,name)),
        }
    }

    pub fn getList<'a>( &'a self, name:&'a str ) -> Result<&'a List,String>{
        match self.params.get( name ){
            Some( pv ) => {
                match *pv {
                    ParameterValue::String{ line, ref value } =>
                        Err(format!("Line {} : Parameter \"{}\" is not list", line, name)),
                    ParameterValue::Map( ref map ) =>
                        Err(format!("Line {} : Parameter \"{}\" is not list", map.line, name)),
                    ParameterValue::List( ref list ) =>
                        Ok( list ),
                }
            },
            None=>Err(format!("Line {} : Map has no parameter \"{}\"", self.line,name)),
        }
    }

    pub fn getString<'a>( &'a self, name:&'a str ) -> Result<&'a String,String>{
        match self.params.get( name ){
            Some( pv ) => {
                match *pv {
                    ParameterValue::String{ line, ref value } =>
                        Ok(value),
                    ParameterValue::Map( ref map ) =>
                        Err(format!("Line {} : Parameter \"{}\" is not string", map.line, name)),
                    ParameterValue::List( ref list ) =>
                        Err(format!("Line {} : Parameter \"{}\" is not string", list.line, name)),
                }
            },
            None=>Err(format!("Line {} : Map has no parameter \"{}\"", self.line, name)),
        }
    }

    pub fn getStringAs<'a,T>( &'a self, name:&'a str ) -> Result<T,String> where T: FromStr{
        match self.params.get( name ){
            Some( pv ) => {
                match *pv {
                    ParameterValue::String{ line, ref value } =>{
                        match value.parse::<T>() {
                            Ok( p ) => Ok( p ),
                            Err( e )=>Err( format!("Line {} : Can not parse parameter \"{}\"",line,  name)),
                        }
                    },
                    ParameterValue::Map( ref map ) =>
                        Err(format!("Line {} : Parameter \"{}\" is not string", map.line, name)),
                    ParameterValue::List( ref list ) =>
                        Err(format!("Line {} : Parameter \"{}\" is not string", list.line, name)),
                }
            },
            None=>Err(format!("Line {} : Map has no parameter \"{}\"", self.line, name)),
        }
    }
}

pub struct List{
    line:usize,
    elements:Vec<ParameterValue>,
}

impl List{
    fn parse( cur:& mut Cursor ) -> Result<List, String>{
        let listLine=cur.line;

        let mut elements=Vec::new();

        loop{
            let line=cur.line;

            let elem=match try!( cur.next() ) {
                Lexeme::Bracket( '{') =>
                    ParameterValue::Map( try!(Map::parse( cur, '}' )) ),
                Lexeme::Bracket( '[') =>
                    ParameterValue::List( try!(List::parse( cur )) ),
                Lexeme::String( s ) =>
                    ParameterValue::String{line:line, value: String::from(s) },
                Lexeme::Bracket(']') =>
                    break,
                Lexeme::NewLine | Lexeme::Comma =>
                    return Err( format!("{}Parameter has been missed", cur.printLine()) ),
                _=>
                    return Err( format!("{}Expected string(\"..\"), list([...]), map(_.._) , but {} have been found", cur.printLine(), cur.lex.print()) ),
            };

            elements.push(elem);

            match try!( cur.next() ){
                Lexeme::Bracket(']') =>
                    break,
                Lexeme::NewLine | Lexeme::Comma => {},
                _=>return Err( format!("{}Expected 'new line' , ',' or ']' , but {} have been found", cur.printLine(), cur.lex.print() ) ),
            }
        }

        Ok( List{
                line:listLine,
                elements:elements,
            }
        )
    }

    pub fn iter<'a>( &'a self ) -> Iter<'a,ParameterValue> {
        self.elements.iter()
    }


}

pub fn parse<T,F>( text:&str, process:F) -> Result<T, String> where F:FnOnce(Map) -> Result<T, String>{
    let mut cur=Cursor::new(text);

    let map=try!(Map::parse( &mut cur, '\0' ));

    process(map)
}
