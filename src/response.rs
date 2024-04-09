use chrono::{DateTime, Utc};
use mongodb::bson::{self};
use serde::{Serialize, Deserialize};
use crate::model::Comments;

#[derive(Serialize)]
pub struct GenericResponse {
    pub status: String,
    pub message: String,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct UserResponse {
    pub email: String,
    pub firstName: String,
    pub lastName: String,
}

#[allow(non_snake_case)]
#[derive(Serialize, Deserialize, Debug)]
pub struct UserResponseDel {
    pub email: String,
    pub firstName: String,
    pub lastName: String,
    pub removed: bool,
}

#[derive(Serialize, Debug)]
pub struct UserData {
    pub token: String,
    pub user: UserResponse,
}

#[derive(Serialize, Debug)]
pub struct PostData {
    pub createdAt: String,
    pub userId: String,
    pub firstName: String,
    pub title: String,
    pub content: String,
    pub comments: Vec<Comments>,
    pub upVotes: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PostResponse {
    pub email: String,
    pub firstName: String,
    pub lastName: String,
}

#[derive(Serialize, Debug)]
pub struct SingleUserResponse {
    pub ok: bool,
    pub data: UserData,
}

#[derive(Serialize, Debug)]
pub struct SinglePostResponse {
    pub ok: bool,
    pub data: PostData,
}

#[derive(Serialize, Debug)]
pub struct SingleUserResponseGet {
    pub ok: bool,
    pub data: UserResponse,
}

#[derive(Serialize, Debug)]
pub struct SingleUserResponseDel {
    pub ok: bool,
    pub data: UserResponseDel,
}

#[derive(Serialize, Debug)]
pub struct SinglePostResponseGet {
    pub ok: bool,
    pub data: Vec<PostData>,
}

#[derive(Serialize, Debug)]
pub struct UserListResponse {
    pub status: &'static str,
    pub results: usize,
    pub user: Vec<UserResponse>,
}
