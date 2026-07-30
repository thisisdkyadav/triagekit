import React, { useEffect, useMemo, useState } from 'react';
import { createRoot } from 'react-dom/client';
import {
  AlertTriangle,
  ArrowUpRight,
  Bug,
  CheckCircle2,
  Circle,
  CircleDot,
  Clock,
  HelpCircle,
  Inbox,
  LayoutGrid,
  List,
  LogOut,
  Mail,
  Plus,
  Search,
  Sparkles,
  UserPlus,
  Users,
  XCircle,
} from 'lucide-react';
import {
  Alert,
  Avatar,
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Field,
  Input,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  Sheet,
  SheetClose,
  SheetContent,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  Textarea,
} from './components/ui';
import './styles.css';

// An explicitly-set VITE_API_URL always wins, even when empty. Empty means
// "same origin" (relative /api/... requests), which is what you want when nginx
// serves the SPA and proxies /api to the API on one domain. Only an *unset*
// variable falls back to the local dev server.
const API_URL = import.meta.env.VITE_API_URL ?? 'http://localhost:8080';
const BASE_PATH = import.meta.env.VITE_APP_BASE_PATH || '/issue-management';

const emptyLogin = { email: 'dev@example.com', password: '' };
const emptyDeveloper = { name: '', email: '', password: '', role: 'developer' };
const emptyAllowedUser = { name: '', email: '', externalUserId: '', role: 'user' };
const emptyIssue = { title: '', description: '', issueType: 'bug', priority: 'medium' };
const statuses = ['new', 'triaged', 'in_progress', 'done', 'rejected'];
const issueTypes = ['bug', 'feature', 'question'];
const priorities = ['low', 'medium', 'high', 'urgent'];

const typeMeta = {
  bug: { icon: Bug, label: 'Bug' },
  feature: { icon: Sparkles, label: 'Feature' },
  question: { icon: HelpCircle, label: 'Question' },
};

const statusMeta = {
  new: { icon: Circle, label: 'New' },
  triaged: { icon: CircleDot, label: 'Triaged' },
  in_progress: { icon: Clock, label: 'In Progress' },
  done: { icon: CheckCircle2, label: 'Done' },
  rejected: { icon: XCircle, label: 'Rejected' },
};

const priorityRank = { urgent: 0, high: 1, medium: 2, low: 3 };

function App() {
  const [session, setSession] = useState(() => readSession());
  const [tab, setTab] = useState('issues');
  const [issues, setIssues] = useState([]);
  const [developers, setDevelopers] = useState([]);
  const [allowedUsers, setAllowedUsers] = useState([]);
  const [login, setLogin] = useState(emptyLogin);
  const [newDeveloper, setNewDeveloper] = useState(emptyDeveloper);
  const [newAllowedUser, setNewAllowedUser] = useState(emptyAllowedUser);
  const [newIssue, setNewIssue] = useState(emptyIssue);
  const [selectedIssue, setSelectedIssue] = useState(null);
  const [issueDialogOpen, setIssueDialogOpen] = useState(false);
  const [developerDialogOpen, setDeveloperDialogOpen] = useState(false);
  const [allowedUserDialogOpen, setAllowedUserDialogOpen] = useState(false);
  const [error, setError] = useState('');
  const isDeveloper = session?.user?.role === 'developer' || session?.user?.role === 'owner';

  useEffect(() => {
    exchangeCodeFromUrl(setSession, setError);
  }, []);

  useEffect(() => {
    if (!session) return;
    loadIssues(session).then(setIssues).catch((err) => setError(err.message));
  }, [session]);

  useEffect(() => {
    if (!isDeveloper) return;
    loadDevelopers(session).then(setDevelopers).catch((err) => setError(err.message));
    loadAllowedUsers(session).then(setAllowedUsers).catch((err) => setError(err.message));
  }, [isDeveloper, session]);

  async function handleLogin(event) {
    event.preventDefault();
    setError('');
    try {
      const nextSession = await api('/api/developer/login', {
        method: 'POST',
        body: JSON.stringify(login),
      });
      saveSession(nextSession);
      setSession(nextSession);
    } catch (err) {
      setError(err.message);
    }
  }

  async function handleCreateDeveloper(event) {
    event.preventDefault();
    setError('');
    try {
      const user = await api('/api/developers', {
        method: 'POST',
        body: JSON.stringify(newDeveloper),
        session,
      });
      setDevelopers((current) => [...current, user]);
      setNewDeveloper(emptyDeveloper);
      setDeveloperDialogOpen(false);
    } catch (err) {
      setError(err.message);
    }
  }

  async function handleCreateAllowedUser(event) {
    event.preventDefault();
    setError('');
    try {
      const user = await api('/api/allowed-users', {
        method: 'POST',
        body: JSON.stringify(newAllowedUser),
        session,
      });
      setAllowedUsers((current) => [...current, user]);
      setNewAllowedUser(emptyAllowedUser);
      setAllowedUserDialogOpen(false);
    } catch (err) {
      setError(err.message);
    }
  }

  async function handleCreateIssue(event) {
    event.preventDefault();
    setError('');
    try {
      const issue = await api('/api/issues', {
        method: 'POST',
        body: JSON.stringify({ ...newIssue, sourceUrl: window.location.pathname }),
        session,
      });
      setIssues((current) => [issue, ...current]);
      setNewIssue(emptyIssue);
      setIssueDialogOpen(false);
      setSelectedIssue(issue);
    } catch (err) {
      setError(err.message);
    }
  }

  async function handleStatus(issue, status) {
    try {
      const updated = await api(`/api/issues/${issue.id}/status`, {
        method: 'POST',
        body: JSON.stringify({ status }),
        session,
      });
      setIssues((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      setSelectedIssue((current) => (current && current.id === updated.id ? updated : current));
    } catch (err) {
      setError(err.message);
    }
  }

  function logout() {
    localStorage.removeItem('triagekit_session');
    setSession(null);
    setTab('issues');
  }

  if (!session) {
    return <LoginScreen login={login} setLogin={setLogin} onSubmit={handleLogin} error={error} />;
  }

  return (
    <div className="app">
      <Sidebar
        user={session.user}
        tab={tab}
        setTab={setTab}
        isDeveloper={isDeveloper}
        issueCount={issues.length}
        userCount={allowedUsers.length + developers.length}
        onLogout={logout}
      />

      <main className="main">
        <div className="main__scroll">
          {error && <Alert className="page-alert">{error}</Alert>}

          {tab === 'issues' && (
            <IssuesView
              issues={issues}
              isDeveloper={isDeveloper}
              onSelect={setSelectedIssue}
              onStatusChange={handleStatus}
              newIssueDialog={
                <NewIssueDialog
                  open={issueDialogOpen}
                  onOpenChange={setIssueDialogOpen}
                  issue={newIssue}
                  setIssue={setNewIssue}
                  onSubmit={handleCreateIssue}
                />
              }
            />
          )}

          {tab === 'users' && isDeveloper && (
            <UsersView
              developers={developers}
              allowedUsers={allowedUsers}
              addDeveloperDialog={
                <AddDeveloperDialog
                  open={developerDialogOpen}
                  onOpenChange={setDeveloperDialogOpen}
                  developer={newDeveloper}
                  setDeveloper={setNewDeveloper}
                  onSubmit={handleCreateDeveloper}
                />
              }
              addUserDialog={
                <AddAllowedUserDialog
                  open={allowedUserDialogOpen}
                  onOpenChange={setAllowedUserDialogOpen}
                  user={newAllowedUser}
                  setUser={setNewAllowedUser}
                  onSubmit={handleCreateAllowedUser}
                />
              }
            />
          )}
        </div>
      </main>

      <IssueDetailSheet
        issue={selectedIssue}
        isDeveloper={isDeveloper}
        onOpenChange={(open) => !open && setSelectedIssue(null)}
        onStatusChange={handleStatus}
      />
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Login                                                               */
/* ------------------------------------------------------------------ */

function LoginScreen({ login, setLogin, onSubmit, error }) {
  return (
    <div className="auth">
      <aside className="auth__brand">
        <div className="auth__brandtop">
          <span className="brand-mark brand-mark--lg">TK</span>
          <span className="auth__brandname">TriageKit</span>
        </div>
        <div className="auth__pitch">
          <h1>Every bug, feature, and question — in one calm inbox.</h1>
          <p>A lightweight triage workspace that plugs straight into your app over SSO.</p>
        </div>
        <ul className="auth__features">
          <li><CircleDot size={16} /> Board &amp; list views for fast triage</li>
          <li><Sparkles size={16} /> Bugs, features, and questions in one place</li>
          <li><CheckCircle2 size={16} /> Status updates your team actually sees</li>
        </ul>
        <span className="auth__foot">Internal issue tracker · HMS</span>
      </aside>

      <main className="auth__main">
        <Card className="auth__card">
          <CardHeader className="auth__cardhead">
            <span className="brand-mark auth__cardmark">TK</span>
            <CardTitle>Welcome back</CardTitle>
            <p className="auth__sub">Sign in to your developer account to continue.</p>
          </CardHeader>
          <CardContent>
            <form className="form-stack" onSubmit={onSubmit}>
              {error && <Alert>{error}</Alert>}
              <Field label="Email">
                <Input
                  value={login.email}
                  onChange={(event) => setLogin({ ...login, email: event.target.value })}
                  placeholder="dev@example.com"
                  type="email"
                  required
                />
              </Field>
              <Field label="Password">
                <Input
                  value={login.password}
                  onChange={(event) => setLogin({ ...login, password: event.target.value })}
                  placeholder="••••••••"
                  type="password"
                  required
                />
              </Field>
              <Button type="submit" size="lg">Sign in</Button>
            </form>
          </CardContent>
        </Card>
        <p className="auth__hint">Host-app users are signed in automatically through SSO.</p>
      </main>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Sidebar                                                             */
/* ------------------------------------------------------------------ */

function Sidebar({ user, tab, setTab, isDeveloper, issueCount, userCount, onLogout }) {
  return (
    <aside className="sidebar">
      <div className="sidebar__brand">
        <span className="brand-mark">TK</span>
        <div className="sidebar__brandtext">
          <strong>TriageKit</strong>
          <span>Issue tracker</span>
        </div>
      </div>

      <nav className="sidebar__nav">
        <p className="sidebar__navlabel">Workspace</p>
        <NavItem
          icon={Inbox}
          label="Issues"
          count={issueCount}
          active={tab === 'issues'}
          onClick={() => setTab('issues')}
        />
        {isDeveloper && (
          <NavItem
            icon={Users}
            label="Users"
            count={userCount}
            active={tab === 'users'}
            onClick={() => setTab('users')}
          />
        )}
      </nav>

      <div className="sidebar__foot">
        <div className="account">
          <Avatar name={user.name || user.email} />
          <div className="account__text">
            <strong>{user.name || user.email}</strong>
            <span>{user.email}</span>
          </div>
          <Badge variant={`role-${user.role}`} className="account__role">{label(user.role)}</Badge>
        </div>
        <Button variant="ghost" size="icon" onClick={onLogout} title="Sign out" type="button">
          <LogOut size={17} />
        </Button>
      </div>
    </aside>
  );
}

function NavItem({ icon: Icon, label: text, count, active, onClick }) {
  return (
    <button type="button" className={cnav(active)} onClick={onClick}>
      <Icon size={18} className="navitem__icon" />
      <span className="navitem__label">{text}</span>
      {count > 0 && <span className="navitem__count">{count}</span>}
    </button>
  );
}

function cnav(active) {
  return active ? 'navitem navitem--active' : 'navitem';
}

/* ------------------------------------------------------------------ */
/* Issues view (stats + board / list)                                  */
/* ------------------------------------------------------------------ */

function IssuesView({ issues, isDeveloper, onSelect, onStatusChange, newIssueDialog }) {
  const [view, setView] = useState('board');
  const [query, setQuery] = useState('');
  const [typeFilter, setTypeFilter] = useState('all');

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return issues.filter((issue) => {
      const matchesType = typeFilter === 'all' || issue.issue_type === typeFilter;
      const matchesQuery =
        !q ||
        issue.title.toLowerCase().includes(q) ||
        (issue.description || '').toLowerCase().includes(q) ||
        (issue.created_by?.name || '').toLowerCase().includes(q);
      return matchesType && matchesQuery;
    });
  }, [issues, query, typeFilter]);

  const stats = useMemo(() => getStats(issues), [issues]);

  return (
    <>
      <header className="pagehead">
        <div className="pagehead__titles">
          <h1>Issues</h1>
          <p>Track and triage everything coming from your app.</p>
        </div>
        <div className="pagehead__actions">
          <div className="searchbox">
            <Search size={16} />
            <input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search issues…"
              aria-label="Search issues"
            />
          </div>
          {newIssueDialog}
        </div>
      </header>

      <section className="stats">
        {stats.map((stat) => (
          <StatTile key={stat.key} {...stat} />
        ))}
      </section>

      <div className="filterbar">
        <div className="chips">
          <FilterChip active={typeFilter === 'all'} onClick={() => setTypeFilter('all')}>All</FilterChip>
          {issueTypes.map((type) => {
            const Icon = typeMeta[type].icon;
            return (
              <FilterChip key={type} active={typeFilter === type} onClick={() => setTypeFilter(type)}>
                <Icon size={14} /> {typeMeta[type].label}
              </FilterChip>
            );
          })}
        </div>
        <div className="viewtoggle">
          <button
            type="button"
            className={view === 'board' ? 'viewtoggle__btn is-active' : 'viewtoggle__btn'}
            onClick={() => setView('board')}
          >
            <LayoutGrid size={15} /> Board
          </button>
          <button
            type="button"
            className={view === 'list' ? 'viewtoggle__btn is-active' : 'viewtoggle__btn'}
            onClick={() => setView('list')}
          >
            <List size={15} /> List
          </button>
        </div>
      </div>

      {view === 'board' ? (
        <IssueBoard
          issues={filtered}
          isDeveloper={isDeveloper}
          onSelect={onSelect}
          onStatusChange={onStatusChange}
        />
      ) : (
        <IssueList issues={filtered} onSelect={onSelect} />
      )}
    </>
  );
}

function StatTile({ label: text, value, icon: Icon, tone }) {
  return (
    <Card className="stat">
      <div className={`stat__icon stat__icon--${tone}`}>
        <Icon size={18} />
      </div>
      <div className="stat__body">
        <span className="stat__value">{value}</span>
        <span className="stat__label">{text}</span>
      </div>
    </Card>
  );
}

function FilterChip({ active, onClick, children }) {
  return (
    <button type="button" className={active ? 'chip chip--active' : 'chip'} onClick={onClick}>
      {children}
    </button>
  );
}

function IssueBoard({ issues, isDeveloper, onSelect, onStatusChange }) {
  const [dragId, setDragId] = useState(null);
  const [overCol, setOverCol] = useState(null);

  const grouped = useMemo(() => {
    const map = Object.fromEntries(statuses.map((status) => [status, []]));
    for (const issue of issues) {
      if (map[issue.status]) map[issue.status].push(issue);
    }
    for (const status of statuses) {
      map[status].sort(sortIssues);
    }
    return map;
  }, [issues]);

  function onDrop(status) {
    const issue = issues.find((item) => item.id === dragId);
    if (issue && issue.status !== status) onStatusChange(issue, status);
    setDragId(null);
    setOverCol(null);
  }

  if (issues.length === 0) {
    return <EmptyState icon={Inbox} title="No issues yet" message="Issues submitted from your app will show up here." />;
  }

  return (
    <div className="board">
      {statuses.map((status) => {
        const column = grouped[status];
        const Icon = statusMeta[status].icon;
        return (
          <section
            key={status}
            className={overCol === status ? 'board__col board__col--over' : 'board__col'}
            onDragOver={(event) => {
              if (!isDeveloper || !dragId) return;
              event.preventDefault();
              setOverCol(status);
            }}
            onDragLeave={() => setOverCol((current) => (current === status ? null : current))}
            onDrop={() => isDeveloper && onDrop(status)}
          >
            <header className={`board__colhead board__colhead--${status}`}>
              <Icon size={15} />
              <span className="board__coltitle">{statusMeta[status].label}</span>
              <span className="board__colcount">{column.length}</span>
            </header>
            <div className="board__cards">
              {column.map((issue) => (
                <IssueCard
                  key={issue.id}
                  issue={issue}
                  draggable={isDeveloper}
                  dragging={dragId === issue.id}
                  onSelect={onSelect}
                  onDragStart={(event) => {
                    if (!isDeveloper) return;
                    setDragId(issue.id);
                    event.dataTransfer.effectAllowed = 'move';
                    event.dataTransfer.setData('text/plain', issue.id);
                  }}
                  onDragEnd={() => {
                    setDragId(null);
                    setOverCol(null);
                  }}
                />
              ))}
              {column.length === 0 && <p className="board__empty">Nothing here</p>}
            </div>
          </section>
        );
      })}
    </div>
  );
}

function IssueCard({ issue, draggable, dragging, onSelect, onDragStart, onDragEnd }) {
  const TypeIcon = typeMeta[issue.issue_type]?.icon || Bug;
  return (
    <article
      className={`icard icard--${issue.priority}${dragging ? ' icard--dragging' : ''}${draggable ? ' icard--draggable' : ''}`}
      draggable={draggable}
      onDragStart={onDragStart}
      onDragEnd={onDragEnd}
      onClick={() => onSelect(issue)}
      onKeyDown={(event) => (event.key === 'Enter' || event.key === ' ') && (event.preventDefault(), onSelect(issue))}
      tabIndex={0}
      role="button"
    >
      <div className="icard__top">
        <span className={`typechip typechip--${issue.issue_type}`}>
          <TypeIcon size={13} />
          {typeMeta[issue.issue_type]?.label || label(issue.issue_type)}
        </span>
        <Badge variant={`prio-${issue.priority}`}>{label(issue.priority)}</Badge>
      </div>
      <h3 className="icard__title">{issue.title}</h3>
      <div className="icard__foot">
        <span className="icard__author">
          <Avatar name={issue.created_by?.name} size="sm" />
          {issue.created_by?.name}
        </span>
        <span className="icard__date">{formatDate(issue.created_at)}</span>
      </div>
    </article>
  );
}

function IssueList({ issues, onSelect }) {
  if (issues.length === 0) {
    return <EmptyState icon={Inbox} title="No issues match" message="Try clearing the search or filters." />;
  }
  return (
    <Card className="listcard">
      <Table className="issue-table">
        <TableHeader>
          <TableRow>
            <TableHead>Title</TableHead>
            <TableHead>Type</TableHead>
            <TableHead>Priority</TableHead>
            <TableHead>Status</TableHead>
            <TableHead>Requester</TableHead>
            <TableHead>Created</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {issues.map((issue) => {
            const TypeIcon = typeMeta[issue.issue_type]?.icon || Bug;
            return (
              <TableRow className="clickable-row" key={issue.id} onClick={() => onSelect(issue)} tabIndex={0}>
                <TableCell><strong className="cell-title">{issue.title}</strong></TableCell>
                <TableCell>
                  <span className={`typechip typechip--${issue.issue_type}`}>
                    <TypeIcon size={13} /> {typeMeta[issue.issue_type]?.label || label(issue.issue_type)}
                  </span>
                </TableCell>
                <TableCell><Badge variant={`prio-${issue.priority}`}>{label(issue.priority)}</Badge></TableCell>
                <TableCell><Badge variant={`status-${issue.status}`}>{label(issue.status)}</Badge></TableCell>
                <TableCell>
                  <span className="cell-author">
                    <Avatar name={issue.created_by?.name} size="sm" />
                    {issue.created_by?.name}
                  </span>
                </TableCell>
                <TableCell className="cell-muted">{formatDate(issue.created_at)}</TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Issue detail (slide-over)                                           */
/* ------------------------------------------------------------------ */

function IssueDetailSheet({ issue, isDeveloper, onOpenChange, onStatusChange }) {
  return (
    <Sheet open={Boolean(issue)} onOpenChange={onOpenChange}>
      <SheetContent>
        {issue && (
          <>
            <div className="sheet__head">
              <span className={`typechip typechip--${issue.issue_type}`}>
                {React.createElement(typeMeta[issue.issue_type]?.icon || Bug, { size: 13 })}
                {typeMeta[issue.issue_type]?.label || label(issue.issue_type)}
              </span>
              <h2 className="sheet__title">{issue.title}</h2>
              <div className="sheet__badges">
                <Badge variant={`status-${issue.status}`}>{label(issue.status)}</Badge>
                <Badge variant={`prio-${issue.priority}`}>{label(issue.priority)}</Badge>
                <span className="sheet__date">{formatDate(issue.created_at)}</span>
              </div>
            </div>

            <div className="sheet__section">
              <h4 className="sheet__label">Description</h4>
              <p className="sheet__desc">{issue.description || 'No details were added.'}</p>
            </div>

            <div className="sheet__section">
              <h4 className="sheet__label">Reported by</h4>
              <div className="reporter">
                <Avatar name={issue.created_by?.name} />
                <div className="reporter__text">
                  <strong>{issue.created_by?.name}</strong>
                  <span><Mail size={13} /> {issue.created_by?.email}</span>
                </div>
                <Badge variant={`role-${issue.created_by?.role}`}>{label(issue.created_by?.role)}</Badge>
              </div>
            </div>

            {issue.source_url && (
              <div className="sheet__section">
                <h4 className="sheet__label">Source</h4>
                <span className="sourcelink"><ArrowUpRight size={14} /> {issue.source_url}</span>
              </div>
            )}

            {isDeveloper && (
              <div className="sheet__section">
                <h4 className="sheet__label">Update status</h4>
                <div className="statusbar">
                  {statuses.map((status) => {
                    const Icon = statusMeta[status].icon;
                    const active = issue.status === status;
                    return (
                      <button
                        key={status}
                        type="button"
                        className={`statuspill statuspill--${status}${active ? ' is-active' : ''}`}
                        onClick={() => !active && onStatusChange(issue, status)}
                      >
                        <Icon size={14} /> {statusMeta[status].label}
                      </button>
                    );
                  })}
                </div>
              </div>
            )}

            <div className="sheet__actions">
              <SheetClose asChild>
                <Button variant="outline" type="button">Close</Button>
              </SheetClose>
            </div>
          </>
        )}
      </SheetContent>
    </Sheet>
  );
}

/* ------------------------------------------------------------------ */
/* Users view                                                          */
/* ------------------------------------------------------------------ */

function UsersView({ developers, allowedUsers, addDeveloperDialog, addUserDialog }) {
  return (
    <>
      <header className="pagehead">
        <div className="pagehead__titles">
          <h1>Users</h1>
          <p>Manage developer accounts and the people allowed to submit issues.</p>
        </div>
        <div className="pagehead__actions">
          {addUserDialog}
          {addDeveloperDialog}
        </div>
      </header>

      <section className="users-grid">
        <UsersPanel
          title="Developers"
          subtitle="Owners and developers who triage issues."
          icon={Users}
          users={developers}
          columns={['name', 'email', 'role']}
        />
        <UsersPanel
          title="Allowed users"
          subtitle="Host-app users who can submit through SSO."
          icon={UserPlus}
          users={allowedUsers}
          columns={['name', 'email', 'externalUserId']}
        />
      </section>
    </>
  );
}

function UsersPanel({ title, subtitle, icon: Icon, users, columns }) {
  return (
    <Card className="userspanel">
      <CardHeader className="userspanel__head">
        <div className="userspanel__heading">
          <span className="userspanel__icon"><Icon size={16} /></span>
          <div>
            <CardTitle>{title}</CardTitle>
            <p className="userspanel__sub">{subtitle}</p>
          </div>
        </div>
        <Badge variant="muted">{users.length}</Badge>
      </CardHeader>
      <CardContent className="userspanel__content">
        {users.length === 0 ? (
          <EmptyState icon={Icon} title="No one yet" message="Add the first person to get started." compact />
        ) : (
          <ul className="userlist">
            {users.map((user) => (
              <li className="userrow" key={user.id}>
                <Avatar name={user.name} />
                <div className="userrow__text">
                  <strong>{user.name}</strong>
                  <span>{user.email}</span>
                </div>
                <span className="userrow__meta">
                  {columns.includes('role') ? (
                    <Badge variant={`role-${user.role}`}>{label(user.role)}</Badge>
                  ) : (
                    user.externalUserId && <span className="userrow__ext">{user.externalUserId}</span>
                  )}
                </span>
              </li>
            ))}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/* Dialogs                                                             */
/* ------------------------------------------------------------------ */

function AddDeveloperDialog({ open, onOpenChange, developer, setDeveloper, onSubmit }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogTrigger asChild>
        <Button type="button">
          <Plus size={16} />
          Developer
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add developer</DialogTitle>
          <DialogDescription>Create an owner or developer account.</DialogDescription>
        </DialogHeader>
        <form className="form-stack" onSubmit={onSubmit}>
          <Field label="Name">
            <Input value={developer.name} onChange={(event) => setDeveloper({ ...developer, name: event.target.value })} required />
          </Field>
          <Field label="Email">
            <Input value={developer.email} onChange={(event) => setDeveloper({ ...developer, email: event.target.value })} type="email" required />
          </Field>
          <Field label="Password">
            <Input value={developer.password} onChange={(event) => setDeveloper({ ...developer, password: event.target.value })} type="password" required />
          </Field>
          <Field label="Role">
            <ValueSelect value={developer.role} onChange={(role) => setDeveloper({ ...developer, role })} values={['developer', 'owner']} />
          </Field>
          <div className="dialog-actions">
            <DialogClose asChild>
              <Button variant="outline" type="button">Cancel</Button>
            </DialogClose>
            <Button type="submit">Create developer</Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function AddAllowedUserDialog({ open, onOpenChange, user, setUser, onSubmit }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogTrigger asChild>
        <Button variant="outline" type="button">
          <Plus size={16} />
          User
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add user</DialogTitle>
          <DialogDescription>Allow a host-app user to submit issues through SSO.</DialogDescription>
        </DialogHeader>
        <form className="form-stack" onSubmit={onSubmit}>
          <Field label="Name">
            <Input value={user.name} onChange={(event) => setUser({ ...user, name: event.target.value })} required />
          </Field>
          <Field label="Email">
            <Input value={user.email} onChange={(event) => setUser({ ...user, email: event.target.value })} type="email" required />
          </Field>
          <Field label="External ID" hint="The user's id in your host app (optional).">
            <Input value={user.externalUserId} onChange={(event) => setUser({ ...user, externalUserId: event.target.value })} />
          </Field>
          <div className="dialog-actions">
            <DialogClose asChild>
              <Button variant="outline" type="button">Cancel</Button>
            </DialogClose>
            <Button type="submit">Create user</Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

function NewIssueDialog({ open, onOpenChange, issue, setIssue, onSubmit }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogTrigger asChild>
        <Button type="button">
          <Plus size={16} />
          New issue
        </Button>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>New issue</DialogTitle>
          <DialogDescription>Submit a bug, feature, or question.</DialogDescription>
        </DialogHeader>
        <form className="form-stack" onSubmit={onSubmit}>
          <Field label="Title">
            <Input value={issue.title} onChange={(event) => setIssue({ ...issue, title: event.target.value })} placeholder="Short summary" required />
          </Field>
          <Field label="Details">
            <Textarea value={issue.description} onChange={(event) => setIssue({ ...issue, description: event.target.value })} rows={5} placeholder="What happened, and what did you expect?" />
          </Field>
          <div className="form-grid">
            <Field label="Type">
              <ValueSelect value={issue.issueType} onChange={(issueType) => setIssue({ ...issue, issueType })} values={issueTypes} />
            </Field>
            <Field label="Priority">
              <ValueSelect value={issue.priority} onChange={(priority) => setIssue({ ...issue, priority })} values={priorities} />
            </Field>
          </div>
          <div className="dialog-actions">
            <DialogClose asChild>
              <Button variant="outline" type="button">Cancel</Button>
            </DialogClose>
            <Button type="submit">Create issue</Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}

/* ------------------------------------------------------------------ */
/* Small shared bits                                                   */
/* ------------------------------------------------------------------ */

function ValueSelect({ value, onChange, values }) {
  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger>{label(value)}</SelectTrigger>
      <SelectContent>
        {values.map((item) => (
          <SelectItem key={item} value={item}>{label(item)}</SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function EmptyState({ icon: Icon, title, message, compact }) {
  return (
    <div className={compact ? 'empty empty--compact' : 'empty'}>
      <span className="empty__icon"><Icon size={compact ? 20 : 24} /></span>
      <strong>{title}</strong>
      <span>{message}</span>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Data + helpers                                                      */
/* ------------------------------------------------------------------ */

function getStats(issues) {
  const count = (predicate) => issues.filter(predicate).length;
  return [
    { key: 'total', label: 'Total issues', value: issues.length, icon: Inbox, tone: 'primary' },
    { key: 'open', label: 'Open', value: count((i) => ['new', 'triaged', 'in_progress'].includes(i.status)), icon: CircleDot, tone: 'amber' },
    { key: 'done', label: 'Resolved', value: count((i) => i.status === 'done'), icon: CheckCircle2, tone: 'green' },
    { key: 'urgent', label: 'Needs attention', value: count((i) => ['urgent', 'high'].includes(i.priority)), icon: AlertTriangle, tone: 'red' },
  ];
}

function sortIssues(a, b) {
  const rank = (priorityRank[a.priority] ?? 9) - (priorityRank[b.priority] ?? 9);
  if (rank !== 0) return rank;
  return new Date(b.created_at) - new Date(a.created_at);
}

async function exchangeCodeFromUrl(setSession, setError) {
  const url = new URL(window.location.href);
  const code = url.searchParams.get('code');
  if (!window.location.pathname.endsWith('/sso/callback') || !code) return;

  try {
    const session = await api('/api/sso/exchange', {
      method: 'POST',
      body: JSON.stringify({ code }),
    });
    saveSession(session);
    setSession(session);
    window.history.replaceState({}, '', `${BASE_PATH}${session.returnTo || '/issues'}`);
  } catch (err) {
    setError(err.message);
  }
}

function loadIssues(session) {
  return api('/api/issues', { session });
}

function loadDevelopers(session) {
  return api('/api/developers', { session });
}

function loadAllowedUsers(session) {
  return api('/api/allowed-users', { session });
}

async function api(path, options = {}) {
  const headers = {
    'Content-Type': 'application/json',
    ...(options.headers || {}),
  };

  if (options.session?.token) {
    headers.Authorization = `Bearer ${options.session.token}`;
  }

  const response = await fetch(`${API_URL}${path}`, { ...options, headers });
  const data = await response.json();

  if (!response.ok) {
    throw new Error(data?.error?.message || 'Request failed');
  }

  return data;
}

function readSession() {
  try {
    return JSON.parse(localStorage.getItem('triagekit_session'));
  } catch {
    return null;
  }
}

function saveSession(session) {
  localStorage.setItem('triagekit_session', JSON.stringify(session));
}

function label(value) {
  return String(value || '')
    .replaceAll('_', ' ')
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function formatDate(value) {
  if (!value) return '—';
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '—';
  return date.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

createRoot(document.getElementById('root')).render(<App />);
