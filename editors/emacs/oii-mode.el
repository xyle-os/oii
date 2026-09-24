;;; oii-mode. Minimal major mode.
(define-derived-mode oii-mode prog-mode "OII"
  "Brackets scope."
  (setq-local comment-start "// ")
  (setq-local comment-start-skip "//+\\s-*")
  (setq-local indent-tabs-mode nil)
  (setq-local tab-width 2)
  (setq font-lock-defaults '(oii-font-lock-keywords)))

(defvar oii-font-lock-keywords
  (list
   '("//.*$" . font-lock-comment-face)
   '("/\\*.*\\*/" . font-lock-comment-face)
   '("\\bimpt\\b" . font-lock-keyword-face)
   '("\\b\\(fun\\|desc\\|let\\|if\\|else\\|while\\|for\\|in\\|return\\)\\b" . font-lock-keyword-face)
   '("\\b\\(true\\|false\\|null\\)\\b" . font-lock-constant-face)
   '("#\".*?\"#" . font-lock-string-face)
   '("\"\\(?:\\\\.\\|[^\"\\\\\\n]\\)*\"" . font-lock-string-face)
   '("{[A-Za-z_][A-Za-z0-9_]*}" . font-lock-variable-name-face)
   '("\\b[+-]?0[xX][0-9A-Fa-f_]+\\b" . font-lock-constant-face)
   '("\\b[+-]?0[oO][0-7_]+\\b" . font-lock-constant-face)
   '("\\b[+-]?0[bB][01_]+\\b" . font-lock-constant-face)
   '("\\b[+-]?\\([0-9][0-9_]*\\.[0-9][0-9_]*\\|[0-9][0-9_]*[eE][+-]?[0-9]+\\|\\.[0-9]+\\)" . font-lock-constant-face)
   '("\\b[+-]?[0-9][0-9_]*\\b" . font-lock-constant-face)
   '("[{}]" . font-lock-warning-face)))

;;;###autoload
(add-to-list 'auto-mode-alist '("\\.oii\\'" . oii-mode))
(provide 'oii-mode)
