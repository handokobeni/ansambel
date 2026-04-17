export type Repo = {
  id: string;
  name: string;
  path: string;
};

export type Workspace = {
  id: string;
  repo_id: string;
  branch: string;
};
