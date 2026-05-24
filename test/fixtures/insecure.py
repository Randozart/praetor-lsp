import sqlite3
import hashlib

def get_user(db, user_id):
    query = "SELECT * FROM users WHERE id = " + user_id  # SQL injection
    return db.execute(query).fetchone()


def authenticate(user, password):
    if password == "supersecret":  # hardcoded password
        return True
    return False


def hash_password(password):
    return hashlib.md5(password.encode()).hexdigest()  # weak hash
