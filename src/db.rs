use crate::error::MyError;
use crate::response::{
    GenericResponse, UserData, UserListResponse,
    UserResponse, SingleUserResponse, SingleUserResponseGet,
    SinglePostResponse, PostData, SinglePostResponseGet,
    SingleUserResponseDel, UserResponseDel,
};
use crate::{
    error::MyError::*, model::{UserModel, PostModel, Claims, Comments},
    schema::{CreateUserSchema, UpdateUserSchema, CreatePostSchema, LoginSchema},
};
use chrono::prelude::*;
use futures::StreamExt;
use mongodb::bson::{doc, oid::ObjectId, Document};
use mongodb::options::{
    FindOneAndUpdateOptions, FindOptions, IndexOptions, ReturnDocument
};
use mongodb::{bson, options::ClientOptions, Client, Collection, IndexModel};
use std::str::FromStr;
use jsonwebtoken::{
    encode, decode, Header, Algorithm,
    EncodingKey, DecodingKey, Validation
};
use serde_json::{json, Value};
use std::{time::{SystemTime, UNIX_EPOCH}};
use crypto::{digest::Digest, sha3::Sha3};
use axum::{
    Json,
    http::header::HeaderMap,
};

#[derive(Clone, Debug)]
pub struct DB {
    pub user_collection: Collection<UserModel>,
    pub user_collection_doc: Collection<Document>,
    pub post_collection: Collection<PostModel>,
    pub post_collection_doc: Collection<Document>,
}

type Result<T> = std::result::Result<T, MyError>;
const SECRET_KEY: &[u8] = b"LgKp";

impl DB {
    pub async fn init() -> Result<Self> {
        let mongodb_uri = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set.");
        let database_name =
            std::env::var("MONGO_DB_DATABASE").expect("MONGO_DB_DATABASE must be set.");
        let user_collection_name =
            std::env::var("MONGODB_USER_COLLECTION").expect("MONGODB_USER_COLLECTION must be set.");
        let post_collection_name =
            std::env::var("MONGODB_POST_COLLECTION").expect("MONGODB_POST_COLLECTION must be set.");

        let mut client_options = ClientOptions::parse(mongodb_uri).await?;
        client_options.app_name = Some(database_name.to_string());

        let client = Client::with_options(client_options)?;
        let database = client.database(database_name.as_str());

        let user_collection = database.collection(user_collection_name.as_str());
        let post_collection = database.collection(post_collection_name.as_str());
        let user_collection_doc =
            database.collection::<Document>(user_collection_name.as_str());
        let post_collection_doc =
            database.collection::<Document>(post_collection_name.as_str());

        println!("✅ Database connected successfully");

        Ok(Self {
            user_collection,
            user_collection_doc,
            post_collection,
            post_collection_doc
        })
    }

    pub async fn login(&self, body: &LoginSchema)
        -> Result<SingleUserResponse> {
            if body.email.is_empty() || body.password.is_empty() {
                return Err(InvalidIdentifiants());
            }

            let user_doc = match self
                .user_collection
                .find_one(doc! {"email": body.email.to_string()}, None)
                .await
                {
                    Ok(Some(doc)) => doc,
                    Ok(None) => return Err(InvalidIdentifiants()),
                    Err(e) => return Err(InvalidIdentifiants()),
                };
            let password = self.hash_string(body.password.to_string());
            if password != user_doc.password {
                return Err(InvalidIdentifiants());
            }
            let jwt = self.generate_token(&user_doc)?;
            Ok(SingleUserResponse {
                ok: true,
                data: UserData {
                    token: jwt,
                    user: self.doc_to_user(&user_doc)?,
                }
            })
    }
    
    pub async fn create_user(&self, body: &CreateUserSchema)
        -> Result<SingleUserResponse> {
            if body.email.is_empty() || body.firstName.is_empty() ||
                body.lastName.is_empty() || body.password.is_empty() {
                    return Err(InvalidIdentifiants());
                }
            let document = self.create_user_document(body)?;

            let options = IndexOptions::builder().unique(true).build();
            let index = IndexModel::builder()
                .keys(doc! {"email": 1})
                .options(options)
                .build();

            match self.user_collection.create_index(index, None).await {
                Ok(_) => {}
                Err(e) => return Err(MongoQueryError(e)),
            };
            let insert_result = match self.user_collection_doc.insert_one(&document, None).await {
                Ok(result) => result,
                Err(e) => {
                    if e.to_string()
                        .contains("E11000 duplicate key error collection")
                        {
                            return Err(MongoDuplicateError(e));
                        }
                    return Err(MongoQueryError(e));
                }
            };
            let new_id = insert_result
                .inserted_id
                .as_object_id()
                .expect("issue with new _id");

            let user_doc = match self
                .user_collection
                .find_one(doc! {"_id": new_id}, None)
                .await
                {
                    Ok(Some(doc)) => doc,
                    Ok(None) => return Err(NotFoundError(new_id.to_string())),
                    Err(e) => return Err(MongoQueryError(e)),
                };
            Ok(SingleUserResponse {
                ok: true,
                data: UserData {
                    token: self.generate_token(&user_doc)?,
                    user: self.doc_to_user(&user_doc)?,
                },
            })
    }

    pub async fn post(&self, headers: &HeaderMap, body: &CreatePostSchema)
        -> Result<SinglePostResponse> {
            let authorization_header = match headers.get("Authorization") {
                Some(value) => value,
                None => return Err(JwtNotFoundError("".to_string())),
            };
            let header_str = match authorization_header.to_str() {
                Ok(value) => value,
                Err(_) => return Err(JwtNotFoundError("".to_string())),
            };
            let jwt = header_str.trim_start_matches("Bearer ");
            let obj_id = match self.id_from_jwt(jwt.to_string()) {
                Some(value) => value,
                None => return Err(JwtNotFoundError("".to_string())),
            };

            let document = self.create_post_document(body, obj_id).await?;

            let insert_result = match self.post_collection_doc.insert_one(&document, None).await {
                Ok(result) => result,
                Err(e) => {
                    if e.to_string()
                        .contains("E11000 duplicate key error collection")
                        {
                            return Err(MongoDuplicateError(e));
                        }
                    return Err(MongoQueryError(e));
                }
            };

            let new_id = insert_result
                .inserted_id
                .as_object_id()
                .expect("issue with new _id");

            let post_doc = match self
                .post_collection
                .find_one(doc! {"_id": new_id}, None)
                .await
                {
                    Ok(Some(doc)) => doc,
                    Ok(None) => return Err(NotFoundError(new_id.to_string())),
                    Err(e) => return Err(MongoQueryError(e)),
                };
            Ok(SinglePostResponse {
                ok: true,
                data: PostData {
                    createdAt: post_doc.createdAt.to_string(),
                    userId: new_id.to_string(),
                    firstName: post_doc.firstName,
                    title: post_doc.title,
                    content: post_doc.content,
                    comments: post_doc.comments,
                    upVotes: post_doc.upVotes,
                },
            })
    }

    pub async fn connected(&self, headers: &HeaderMap)
        -> Result<SingleUserResponseGet> {
            let authorization_header = match headers.get("Authorization") {
                Some(value) => value,
                None => return Err(JwtNotFoundError("".to_string())),
            };
            let header_str = match authorization_header.to_str() {
                Ok(value) => value,
                Err(_) => return Err(JwtNotFoundError("".to_string())),
            };
            let jwt = header_str.trim_start_matches("Bearer ");
            let obj_id = match self.id_from_jwt(jwt.to_string()) {
                Some(value) => value,
                None => return Err(JwtNotFoundError("".to_string())),
            };
            let user_doc = match self
                .user_collection
                .find_one(doc! {"_id": obj_id}, None)
                .await
                {
                    Ok(Some(doc)) => doc,
                    Ok(None) => return Err(NotFoundError(obj_id.to_string())),
                    Err(e) => return Err(MongoQueryError(e)),
                };
            Ok(SingleUserResponseGet {
                ok: true,
                data: UserResponse {
                    email: user_doc.email.to_string(),
                    firstName: user_doc.firstName.to_string(),
                    lastName: user_doc.lastName.to_string(),
                },
            })
    }

    pub async fn get_post(&self, headers: &HeaderMap)
        -> Result<SinglePostResponseGet> {
            let authorization_header = match headers.get("Authorization") {
                Some(value) => value,
                None => return Err(JwtNotFoundError("".to_string())),
            };
            let header_str = match authorization_header.to_str() {
                Ok(value) => value,
                Err(_) => return Err(JwtNotFoundError("".to_string())),
            };
            let jwt = header_str.trim_start_matches("Bearer ");
            let obj_id = match self.id_from_jwt(jwt.to_string()) {
                Some(value) => value,
                None => return Err(JwtNotFoundError("".to_string())),
            };
            let mut cursor = self.post_collection.find(None, None).await?;
            let mut post_list: Vec<PostData> = Vec::new();

            while let Some(result) = cursor.next().await {
                match result {
                    Ok(post) => post_list.push(PostData {
                        createdAt: post.createdAt.to_string(),
                        userId: post.userId,
                        firstName: post.firstName,
                        title: post.title,
                        content: post.content,
                        comments: post.comments,
                        upVotes: post.upVotes,
                    }),
                    Err(e) => return Err(e.into()),
                }
            }
            Ok(SinglePostResponseGet {
                ok: true,
                data: post_list,
            })
    }

    pub async fn get_id_post(&self, headers: &HeaderMap, id: &str)
        -> Result<SinglePostResponseGet> {
            if id == "undefined" {
                return Err(InvalidIdentifiants());
            }
            let authorization_header = match headers.get("Authorization") {
                Some(value) => value,
                None => return Err(JwtNotFoundError("".to_string())),
            };
            let header_str = match authorization_header.to_str() {
                Ok(value) => value,
                Err(_) => return Err(JwtNotFoundError("".to_string())),
            };
            let jwt = header_str.trim_start_matches("Bearer ");
            let obj_id = match self.id_from_jwt(jwt.to_string()) {
                Some(value) => value,
                None => return Err(JwtNotFoundError("".to_string())),
            };
            let mut cursor = self
                .post_collection
                .find(doc !{"userId": obj_id}, None)
                .await?;
            let mut post_list: Vec<PostData> = Vec::new();

            while let Some(result) = cursor.next().await {
                match result {
                    Ok(post) => post_list.push(PostData {
                        createdAt: post.createdAt.to_string(),
                        userId: post.userId,
                        firstName: post.firstName,
                        title: post.title,
                        content: post.content,
                        comments: post.comments,
                        upVotes: post.upVotes,
                    }),
                    Err(e) => return Err(e.into()),
                }
            }
            Ok(SinglePostResponseGet {
                ok: true,
                data: post_list,
            })
    }

    pub async fn get_user_post(&self, headers: &HeaderMap)
        -> Result<SinglePostResponseGet> {
            let authorization_header = match headers.get("Authorization") {
                Some(value) => value,
                None => return Err(JwtNotFoundError("".to_string())),
            };
            let header_str = match authorization_header.to_str() {
                Ok(value) => value,
                Err(_) => return Err(JwtNotFoundError("".to_string())),
            };
            let jwt = header_str.trim_start_matches("Bearer ");
            let obj_id = match self.id_from_jwt(jwt.to_string()) {
                Some(value) => value,
                None => return Err(JwtNotFoundError("".to_string())),
            };
            let mut cursor = self
                .post_collection
                .find(doc !{"userId": obj_id}, None)
                .await?;
            let mut post_list: Vec<PostData> = Vec::new();

            while let Some(result) = cursor.next().await {
                match result {
                    Ok(post) => post_list.push(PostData {
                        createdAt: post.createdAt.to_string(),
                        userId: post.userId,
                        firstName: post.firstName,
                        title: post.title,
                        content: post.content,
                        comments: post.comments,
                        upVotes: post.upVotes,
                    }),
                    Err(e) => return Err(e.into()),
                }
            }
            Ok(SinglePostResponseGet {
                ok: true,
                data: post_list,
            })
    }

    pub async fn remove(&self, headers: &HeaderMap)
        -> Result<SingleUserResponseDel> {
            let authorization_header = match headers.get("Authorization") {
                Some(value) => value,
                None => return Err(JwtNotFoundError("".to_string())),
            };
            let header_str = match authorization_header.to_str() {
                Ok(value) => value,
                Err(_) => return Err(JwtNotFoundError("".to_string())),
            };
            let jwt = header_str.trim_start_matches("Bearer ");
            let obj_id = match self.id_from_jwt(jwt.to_string()) {
                Some(value) => value,
                None => return Err(JwtNotFoundError("".to_string())),
            };
            let filter = doc! {"_id": obj_id };

            let user_doc = match self
                .user_collection
                .find_one(doc! {"_id": obj_id}, None)
                .await
                {
                    Ok(Some(doc)) => doc,
                    Ok(None) => return Err(NotFoundError(obj_id.to_string())),
                    Err(e) => return Err(MongoQueryError(e)),
                };
            let result = self
                .user_collection
                .delete_one(filter, None)
                .await
                .map_err(MongoQueryError)?;

            match result.deleted_count {
                0 => Err(NotFoundError(obj_id.to_string())),
                _ => Ok((SingleUserResponseDel {
                    ok: true,
                    data: UserResponseDel {
                        email: user_doc.email,
                        firstName: user_doc.firstName,
                        lastName: user_doc.lastName,
                        removed: true,
                    }
                })),
            }
    }

    pub async fn edit(&self, headers: &HeaderMap, body: &CreateUserSchema)
        -> Result<SingleUserResponseGet> {
            let authorization_header = match headers.get("Authorization") {
                Some(value) => value,
                None => return Err(JwtNotFoundError("".to_string())),
            };
            let header_str = match authorization_header.to_str() {
                Ok(value) => value,
                Err(_) => return Err(JwtNotFoundError("".to_string())),
            };
            let jwt = header_str.trim_start_matches("Bearer ");
            let obj_id = match self.id_from_jwt(jwt.to_string()) {
                Some(value) => value,
                None => return Err(JwtNotFoundError("".to_string())),
            };

            let new_user = CreateUserSchema {
                email: body.email.to_string(),
                password: self.hash_string(body.password.to_string()),
                firstName: body.firstName.to_string(),
                lastName: body.lastName.to_string(),
            };
            let update = doc! {
                "$set": bson::to_document(&new_user).map_err(MongoSerializeBsonError)?,
            };

            let options = FindOneAndUpdateOptions::builder()
                .return_document(ReturnDocument::After)
                .build();
            if let Some(doc) = self
                .user_collection
                    .find_one_and_update(doc! {"_id": obj_id}, update, options)
                    .await
                    .map_err(MongoQueryError)?
                    {
                        let user = self.doc_to_user(&doc)?;
                        let user_response = SingleUserResponseGet {
                            ok: true,
                            data: UserResponse {
                                email: user.email,
                                firstName: user.firstName,
                                lastName: user.lastName,
                            },
                        };
                        Ok(user_response)
                    } else {
                        Err(NotFoundError(obj_id.to_string()))
                    }
    }

    fn id_from_jwt(&self, jwt: String) -> Option<ObjectId> {
        let key = jsonwebtoken::DecodingKey::from_secret(SECRET_KEY);

        let token_message = jsonwebtoken::decode::<Claims>(&jwt,
            &key, &jsonwebtoken::Validation::new(Algorithm::HS384));

        if token_message.is_err() {
            return Option::None
        }
        let id = token_message.unwrap().claims.name;
        let json_value: Value = serde_json::from_str(&id).unwrap();
        let true_id = json_value["$oid"].as_str().unwrap();
        let obj_id = match ObjectId::parse_str(true_id) {
            Ok(ObjectId) => ObjectId,
            Err(_) => return Option::None,
        };
        Option::Some(obj_id)
    }

    fn is_valid_jwt(&self, jwt: String) -> bool {
        let obj_id = match self.id_from_jwt(jwt.to_string()) {
            Some(value) => value,
            None => return false,
        };
        return true;
    }

    fn doc_to_user(&self, user: &UserModel) -> Result<UserResponse> {
        let user_response = UserResponse {
            email: user.email.to_owned(),
            firstName: user.firstName.to_owned(),
            lastName: user.lastName.to_owned(),
        };

        Ok(user_response)
    }

    fn create_user_document(
        &self,
        body: &CreateUserSchema,
        ) -> Result<bson::Document> {
        let user = CreateUserSchema {
            email: body.email.clone(),
            password: self.hash_string(body.password.clone()),
            firstName: body.firstName.clone(),
            lastName: body.lastName.clone(),
        };
        let serialized_data = bson::to_bson(&user).map_err(MongoSerializeBsonError)?;
        let document = serialized_data.as_document().unwrap();

        let datetime = Utc::now();

        let mut doc_with_dates = doc! {
            "createdAt": datetime,
            "lastUpVote": datetime,
        };
        doc_with_dates.extend(document.clone());

        Ok(doc_with_dates)
    }

    async fn create_post_document(
        &self,
        body: &CreatePostSchema,
        obj_id: ObjectId,
        ) -> Result<bson::Document> {
        let user_doc = match self
            .user_collection
            .find_one(doc! {"_id": obj_id}, None)
            .await
            {
                Ok(Some(doc)) => doc,
                Ok(None) => return Err(NotFoundError(obj_id.to_string())),
                Err(e) => return Err(MongoQueryError(e)),
            };
        let datetime = Utc::now();
        let post = PostModel {
            userId: obj_id.to_string().to_owned(),
            title: body.title.to_owned(),
            content: body.content.to_owned(),
            firstName: user_doc.firstName,
            comments: vec![],
            upVotes: vec![],
            createdAt: datetime,
        };
        let serialized_data = bson::to_bson(&post).map_err(MongoSerializeBsonError)?;
        let document = serialized_data.as_document().unwrap();
        Ok(document.clone())
    }

    fn generate_token(&self, user: &UserModel)
        -> Result<String> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let header = jsonwebtoken::Header::new(Algorithm::HS384);
        let claims = Claims {
            name: json!(user.id).to_string(),
            exp: now + (21 * 3600),
            iat: now,
        };

        Ok(encode(&header, &claims,
        &EncodingKey::from_secret(SECRET_KEY))?)
    }
    fn verify_token(&self, jwt: &str)
        -> bool {
        let jwt_split = jwt.split('.');
        let jwt_split:Vec<_> = jwt_split.collect();

        if jwt_split.len() == 3 {
            let message = jwt_split[0].to_owned() + "." + jwt_split[1];
            let signature = jwt_split[2];
            let key = jsonwebtoken::DecodingKey::from_secret(SECRET_KEY);
            return jsonwebtoken::crypto::verify(signature, message.as_bytes(), &key, Algorithm::HS384).unwrap_or(false)
        }
        false
    }
    fn hash_string(&self, hash_str: String) -> String {
        let mut hasher = Sha3::sha3_384();
        hasher.input(hash_str.as_bytes());
        hasher.result_str()
    }
}

