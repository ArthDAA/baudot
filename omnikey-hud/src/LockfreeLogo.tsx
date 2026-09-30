// Logo Lockfree (branding/lockfree-logo.svg), en couleur du texte courant
// pour suivre les thèmes clair et sombre.
export function LockfreeLogo({ className }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 688 637" role="img" aria-label="Lockfree">
      <path
        fill="currentColor"
        transform="translate(-3606.2913 191.4027)"
        d="m 3606.2913,127.09573 318.4984,-318.49844 v 54.0014 54.001386 l -210,209.992884 210.0175,210.02455 -0.2587,54.24091 -0.2588,54.24091 z m 369.4984,-107.495694 213.004,212.996484 -106.004,0.51752 v 105.48949 l -107,106.98598 z m -0.017,-102.02454 0.2587,-54.240896 0.2587,-54.2409 317.9977,318.00282 h -109.0012 z"
      />
    </svg>
  );
}
