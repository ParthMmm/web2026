interface ImportMetaEnv {
  /** The public gallery's base URL, without a trailing slash, such as
      `http://127.0.0.1:8787/gallery`. Unset, /photos/ says the gallery is not
      connected and the masthead does not link to it. */
  readonly PUBLIC_GALLERY_URL?: string;
}
