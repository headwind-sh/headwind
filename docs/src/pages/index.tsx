import {useEffect} from 'react';
import Layout from '@theme/Layout';

export default function Home() {
  useEffect(() => {
    // Redirect to the documentation root immediately
    // Using window.location.replace for better compatibility with GitHub Pages
    if (typeof window !== 'undefined') {
      window.location.replace('/docs');
    }
  }, []);

  // Show a simple loading message while redirecting
  return (
    <Layout>
      <div style={{
        display: 'flex',
        justifyContent: 'center',
        alignItems: 'center',
        minHeight: '80vh',
        fontSize: '1.2rem'
      }}>
        Redirecting to documentation...
      </div>
    </Layout>
  );
}
