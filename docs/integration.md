# Integration Contract

The host application owns login. TriageKit receives a short-lived login code request from the host backend and turns that code into a TriageKit session.

## Create Login Code

```http
POST /api/sso/login-code
Authorization: Bearer <TRIAGEKIT_APP_SECRET>
Content-Type: application/json
```

```json
{
  "externalUserId": "hms-user-123",
  "email": "admin@example.com",
  "name": "Hostel Admin",
  "role": "admin",
  "returnTo": "/issues"
}
```

Response:

```json
{
  "redirectUrl": "http://localhost:5173/issue-management/sso/callback?code=...",
  "expiresAt": "2026-05-14T12:00:00Z"
}
```

The host frontend redirects to `redirectUrl`. The TriageKit frontend exchanges the code and removes it from the URL.

## User Provisioning

The host backend can manage the allow-list of users directly, instead of a TriageKit
developer adding them by hand. All provisioning endpoints are server-to-server and use
the same `TRIAGEKIT_APP_SECRET` bearer token as the login-code endpoint. Users are keyed
by `externalUserId` (the user's id in the host app). If `externalUserId` is omitted on
create, the email is used as the key.

### Create one user

```http
POST /api/sso/users
Authorization: Bearer <TRIAGEKIT_APP_SECRET>
Content-Type: application/json
```

```json
{
  "externalUserId": "hms-user-123",
  "email": "user@example.com",
  "name": "Hostel User",
  "role": "user",
  "upsert": false
}
```

- `email` and `name` are required; `role` defaults to `"user"`.
- With `upsert: false` (default) a duplicate email or `externalUserId` returns `409`.
- With `upsert: true` an existing user (matched by `externalUserId`) is updated instead.

Returns the created/updated user.

### Create or update many (roster sync)

```http
POST /api/sso/users/bulk
Authorization: Bearer <TRIAGEKIT_APP_SECRET>
Content-Type: application/json
```

```json
{
  "users": [
    { "externalUserId": "hms-user-1", "email": "a@example.com", "name": "A" },
    { "externalUserId": "hms-user-2", "email": "b@example.com", "name": "B", "role": "user" }
  ]
}
```

Every user is upserted by `externalUserId` (max 1000 per call). Response:

```json
{ "created": 1, "updated": 1, "users": [ /* AllowedUser objects */ ] }
```

### Edit a user

```http
PATCH /api/sso/users/<externalUserId>
Authorization: Bearer <TRIAGEKIT_APP_SECRET>
Content-Type: application/json
```

```json
{ "email": "new@example.com", "name": "New Name", "role": "user" }
```

Partial update — send only the fields that change. Returns the updated user, or `404`
if no user has that `externalUserId`.

### Delete a user

```http
DELETE /api/sso/users/<externalUserId>
Authorization: Bearer <TRIAGEKIT_APP_SECRET>
```

Returns `{ "ok": true, "externalUserId": "<externalUserId>" }`, or `404` if not found.

